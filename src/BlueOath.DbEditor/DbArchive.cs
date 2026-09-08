using Microsoft.Data.Sqlite;
using System.Text.Encodings.Web;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace BlueOath.DbEditor;

public sealed record DbRepackResult(IReadOnlyList<string> Files, IReadOnlyList<string> Backups);

public static class DbArchive
{
    private const string ArchiveFormat = "BlueOath.DB.Archive";
    private const int ArchiveVersion = 1;
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        WriteIndented = true,
        Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping,
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull
    };

    public static IReadOnlyList<string> Unpack(IEnumerable<string> dbPaths, string outputDirectory)
    {
        var paths = dbPaths.Select(Path.GetFullPath).Distinct(StringComparer.OrdinalIgnoreCase).ToArray();
        if (paths.Length == 0) throw new ArgumentException("No DB files selected.", nameof(dbPaths));
        Directory.CreateDirectory(outputDirectory);

        var outputFiles = new List<string>(paths.Length);
        foreach (var dbPath in paths)
        {
            var sourcePath = DbFileEditor.RequireDatabase(dbPath);
            var rows = DbFileEditor.ReadRawRows(sourcePath)
                .Select(CreateArchiveRow)
                .ToArray();
            var archive = new ArchiveFile
            {
                Format = ArchiveFormat,
                Version = ArchiveVersion,
                XorKey = DbFileEditor.XorKey,
                SourceFile = Path.GetFileName(sourcePath),
                Rows = rows.ToList()
            };
            var outputPath = Path.Combine(Path.GetFullPath(outputDirectory),
                Path.GetFileNameWithoutExtension(sourcePath) + ".json");
            File.WriteAllText(outputPath, JsonSerializer.Serialize(archive, JsonOptions), new UTF8Encoding(false));
            outputFiles.Add(outputPath);
        }
        return outputFiles;
    }

    public static DbRepackResult Repack(string archiveDirectory, string outputDirectory, bool backupExisting = true)
    {
        if (!Directory.Exists(archiveDirectory))
            throw new DirectoryNotFoundException($"Archive directory not found: {archiveDirectory}");
        var archiveFiles = Directory.EnumerateFiles(archiveDirectory, "config_*.json")
            .OrderBy(path => path, StringComparer.OrdinalIgnoreCase)
            .ToArray();
        if (archiveFiles.Length == 0)
            throw new InvalidDataException("No config_*.json archive files found.");

        Directory.CreateDirectory(outputDirectory);
        var outputFiles = new List<string>(archiveFiles.Length);
        var backups = new List<string>();
        foreach (var archivePath in archiveFiles)
        {
            var archive = ReadArchive(archivePath);
            var fileName = ResolveTargetFileName(archive, archivePath);
            var targetPath = Path.Combine(Path.GetFullPath(outputDirectory), fileName);
            var temporaryPath = targetPath + "." + Guid.NewGuid().ToString("N") + ".tmp";
            try
            {
                BuildDatabase(temporaryPath, archive.Rows);
                if (File.Exists(targetPath))
                {
                    if (!backupExisting)
                        throw new IOException($"Target already exists: {targetPath}");
                    backups.Add(DbFileEditor.CreateBackup(targetPath));
                }
                File.Move(temporaryPath, targetPath, overwrite: true);
                outputFiles.Add(targetPath);
            }
            finally
            {
                if (File.Exists(temporaryPath)) File.Delete(temporaryPath);
            }
        }
        return new DbRepackResult(outputFiles, backups);
    }

    private static ArchiveRow CreateArchiveRow(RawDbRecord row)
    {
        var decoded = XorDecode(row.EncodedBytes);
        try
        {
            using var document = JsonDocument.Parse(decoded);
            return new ArchiveRow
            {
                Id = row.Id,
                IndexId = row.IndexId,
                Data = document.RootElement.Clone()
            };
        }
        catch (JsonException)
        {
            return new ArchiveRow
            {
                Id = row.Id,
                IndexId = row.IndexId,
                EncodedBase64 = Convert.ToBase64String(row.EncodedBytes)
            };
        }
    }

    private static ArchiveFile ReadArchive(string path)
    {
        var archive = JsonSerializer.Deserialize<ArchiveFile>(File.ReadAllText(path))
            ?? throw new InvalidDataException($"Empty archive: {path}");
        if (archive.Format != ArchiveFormat || archive.Version != ArchiveVersion || archive.XorKey != DbFileEditor.XorKey)
            throw new InvalidDataException($"Unsupported DB archive: {path}");
        if (archive.Rows.Count == 0)
            throw new InvalidDataException($"Archive has no rows: {path}");
        return archive;
    }

    private static string ResolveTargetFileName(ArchiveFile archive, string archivePath)
    {
        var fileName = string.IsNullOrWhiteSpace(archive.SourceFile)
            ? Path.GetFileNameWithoutExtension(archivePath) + ".db"
            : Path.GetFileName(archive.SourceFile);
        if (!fileName.StartsWith("config_", StringComparison.OrdinalIgnoreCase) ||
            !fileName.EndsWith(".db", StringComparison.OrdinalIgnoreCase))
            throw new InvalidDataException($"Invalid target DB name: {fileName}");
        return fileName;
    }

    private static void BuildDatabase(string path, IReadOnlyList<ArchiveRow> rows)
    {
        var ids = new HashSet<string>(StringComparer.Ordinal);
        using var connection = new SqliteConnection(new SqliteConnectionStringBuilder
        {
            DataSource = path,
            Mode = SqliteOpenMode.ReadWriteCreate,
            Pooling = false
        }.ConnectionString);
        connection.Open();
        using var transaction = connection.BeginTransaction();
        using (var ddl = connection.CreateCommand())
        {
            ddl.Transaction = transaction;
            ddl.CommandText = "CREATE TABLE DBObject(id varchar primary key not null, indexid varchar, jsonbytes blob); CREATE INDEX DBObject_indexid ON DBObject(indexid);";
            ddl.ExecuteNonQuery();
        }
        using var insert = connection.CreateCommand();
        insert.Transaction = transaction;
        insert.CommandText = "INSERT INTO DBObject(id, indexid, jsonbytes) VALUES($id, $indexid, $jsonbytes)";
        var idParameter = insert.Parameters.Add("$id", SqliteType.Text);
        var indexParameter = insert.Parameters.Add("$indexid", SqliteType.Text);
        var jsonParameter = insert.Parameters.Add("$jsonbytes", SqliteType.Blob);
        foreach (var row in rows)
        {
            if (string.IsNullOrWhiteSpace(row.Id) || !ids.Add(row.Id))
                throw new InvalidDataException($"Duplicate or empty row id: {row.Id}");
            idParameter.Value = row.Id;
            indexParameter.Value = (object?)row.IndexId ?? DBNull.Value;
            jsonParameter.Value = EncodeArchiveRow(row);
            insert.ExecuteNonQuery();
        }
        transaction.Commit();
    }

    private static byte[] EncodeArchiveRow(ArchiveRow row)
    {
        if (row.Data.HasValue)
            return DbFileEditor.EncodeJson(row.Data.Value.GetRawText());
        if (string.IsNullOrWhiteSpace(row.EncodedBase64))
            throw new InvalidDataException($"Row has neither data nor encodedBase64: {row.Id}");
        try
        {
            return Convert.FromBase64String(row.EncodedBase64);
        }
        catch (FormatException exception)
        {
            throw new InvalidDataException($"Invalid encodedBase64 for row {row.Id}", exception);
        }
    }

    private static byte[] XorDecode(byte[] encoded)
    {
        var decoded = encoded.ToArray();
        for (var index = 0; index < decoded.Length; index++) decoded[index] ^= DbFileEditor.XorKey;
        return decoded;
    }

    private sealed class ArchiveFile
    {
        public string Format { get; set; } = string.Empty;
        public int Version { get; set; }
        public int XorKey { get; set; }
        public string SourceFile { get; set; } = string.Empty;
        public List<ArchiveRow> Rows { get; set; } = [];
    }

    private sealed class ArchiveRow
    {
        public string Id { get; set; } = string.Empty;
        public string? IndexId { get; set; }
        public JsonElement? Data { get; set; }
        public string? EncodedBase64 { get; set; }
    }
}
