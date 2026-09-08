using Microsoft.Data.Sqlite;
using System.Text;
using System.Text.Json;

namespace BlueOath.DbEditor;

public sealed record DbRecord(
    string Id,
    string? IndexId,
    string JsonText,
    int EncodedSize,
    bool IsEditable,
    string? Error);

public sealed record DbDocument(string Path, IReadOnlyList<DbRecord> Records)
{
    public int EditableCount => Records.Count(record => record.IsEditable);
}

public static class DbFileEditor
{
    public const byte XorKey = 0x55;

    private static readonly Encoding Utf8 = new UTF8Encoding(encoderShouldEmitUTF8Identifier: false);

    public static IReadOnlyList<string> EnumerateDatabases(string configRoot)
    {
        if (!Directory.Exists(configRoot))
            return [];

        return Directory.EnumerateFiles(configRoot, "config_*.db")
            .Where(HasDBObjectTable)
            .OrderBy(path => path, StringComparer.OrdinalIgnoreCase)
            .Select(Path.GetFullPath)
            .ToArray();
    }

    public static DbDocument Load(string dbPath)
    {
        var fullPath = RequireDatabase(dbPath);
        var records = new List<DbRecord>();
        using var connection = Open(fullPath, SqliteOpenMode.ReadOnly);
        using var command = connection.CreateCommand();
        command.CommandText = "SELECT id, indexid, jsonbytes FROM DBObject ORDER BY rowid";
        using var reader = command.ExecuteReader();
        while (reader.Read())
        {
            var id = reader.IsDBNull(0) ? string.Empty : Convert.ToString(reader.GetValue(0)) ?? string.Empty;
            var indexId = reader.IsDBNull(1) ? null : Convert.ToString(reader.GetValue(1));
            var encoded = ReadBytes(reader, 2);
            var decoded = DecodeBytes(encoded);
            var text = Utf8.GetString(decoded);
            var isMetadata = string.Equals(id, "nill", StringComparison.OrdinalIgnoreCase);
            if (isMetadata)
            {
                records.Add(new DbRecord(id, indexId, text, encoded.Length, false, "Metadata row"));
                continue;
            }

            try
            {
                using var document = JsonDocument.Parse(decoded);
                records.Add(new DbRecord(id, indexId, text, encoded.Length, true, null));
            }
            catch (JsonException exception)
            {
                records.Add(new DbRecord(id, indexId, text, encoded.Length, false, exception.Message));
            }
        }

        return new DbDocument(fullPath, records);
    }

    public static string Save(string dbPath, string id, string? indexId, string jsonText)
    {
        ValidateEditableId(id);
        ValidateJson(jsonText);
        var fullPath = RequireDatabase(dbPath);
        var backup = CreateBackup(fullPath);
        using var connection = Open(fullPath, SqliteOpenMode.ReadWrite);
        using var transaction = connection.BeginTransaction();
        using var command = connection.CreateCommand();
        command.Transaction = transaction;
        command.CommandText = """
            UPDATE DBObject
            SET jsonbytes = $jsonbytes
            WHERE id = $id AND ((indexid = $indexid) OR (indexid IS NULL AND $indexid IS NULL));
            """;
        command.Parameters.AddWithValue("$jsonbytes", EncodeJson(jsonText));
        command.Parameters.AddWithValue("$id", id);
        command.Parameters.AddWithValue("$indexid", (object?)indexId ?? DBNull.Value);
        if (command.ExecuteNonQuery() != 1)
            throw new InvalidOperationException($"Row not found: id={id}, indexid={indexId ?? "NULL"}");
        transaction.Commit();
        return backup;
    }

    public static string Add(string dbPath, string id, string? indexId, string jsonText)
    {
        ValidateEditableId(id);
        ValidateJson(jsonText);
        var fullPath = RequireDatabase(dbPath);
        var backup = CreateBackup(fullPath);
        using var connection = Open(fullPath, SqliteOpenMode.ReadWrite);
        using var transaction = connection.BeginTransaction();
        using var command = connection.CreateCommand();
        command.Transaction = transaction;
        command.CommandText = "INSERT INTO DBObject(id, indexid, jsonbytes) VALUES($id, $indexid, $jsonbytes)";
        command.Parameters.AddWithValue("$id", id);
        command.Parameters.AddWithValue("$indexid", (object?)indexId ?? DBNull.Value);
        command.Parameters.AddWithValue("$jsonbytes", EncodeJson(jsonText));
        command.ExecuteNonQuery();
        transaction.Commit();
        return backup;
    }

    public static string Delete(string dbPath, string id)
    {
        ValidateEditableId(id);
        var fullPath = RequireDatabase(dbPath);
        var backup = CreateBackup(fullPath);
        using var connection = Open(fullPath, SqliteOpenMode.ReadWrite);
        using var transaction = connection.BeginTransaction();
        using var command = connection.CreateCommand();
        command.Transaction = transaction;
        command.CommandText = "DELETE FROM DBObject WHERE id = $id";
        command.Parameters.AddWithValue("$id", id);
        if (command.ExecuteNonQuery() != 1)
            throw new InvalidOperationException($"Row not found: id={id}");
        transaction.Commit();
        return backup;
    }

    public static string FormatJson(string jsonText)
    {
        using var document = JsonDocument.Parse(jsonText);
        return JsonSerializer.Serialize(document.RootElement, new JsonSerializerOptions { WriteIndented = true });
    }

    public static byte[] EncodeJson(string jsonText) =>
        Xor(Utf8.GetBytes(jsonText));

    public static string DecodeJson(byte[] encodedBytes) =>
        Utf8.GetString(Xor(encodedBytes));

    private static void ValidateEditableId(string id)
    {
        if (string.IsNullOrWhiteSpace(id) || string.Equals(id, "nill", StringComparison.OrdinalIgnoreCase))
            throw new ArgumentException("Row id must be non-empty and cannot be nill.", nameof(id));
    }

    private static void ValidateJson(string jsonText)
    {
        if (string.IsNullOrWhiteSpace(jsonText))
            throw new ArgumentException("JSON cannot be empty.", nameof(jsonText));
        using var _ = JsonDocument.Parse(jsonText);
    }

    private static string RequireDatabase(string dbPath)
    {
        var fullPath = Path.GetFullPath(dbPath);
        if (!File.Exists(fullPath))
            throw new FileNotFoundException("Database not found.", fullPath);
        if (!HasDBObjectTable(fullPath))
            throw new InvalidDataException($"No DBObject table in {fullPath}");
        return fullPath;
    }

    private static string CreateBackup(string dbPath)
    {
        var backup = $"{dbPath}.{DateTime.UtcNow:yyyyMMdd-HHmmss-fff}-{Guid.NewGuid():N}.bak";
        File.Copy(dbPath, backup, overwrite: false);
        return backup;
    }

    private static bool HasDBObjectTable(string path)
    {
        try
        {
            using var connection = Open(path, SqliteOpenMode.ReadOnly);
            using var command = connection.CreateCommand();
            command.CommandText = "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'DBObject' LIMIT 1";
            return command.ExecuteScalar() is not null;
        }
        catch (SqliteException)
        {
            return false;
        }
    }

    private static SqliteConnection Open(string path, SqliteOpenMode mode)
    {
        var connection = new SqliteConnection(new SqliteConnectionStringBuilder
        {
            DataSource = path,
            Mode = mode,
            Pooling = false
        }.ConnectionString);
        connection.Open();
        return connection;
    }

    private static byte[] ReadBytes(SqliteDataReader reader, int ordinal)
    {
        if (reader.IsDBNull(ordinal)) return [];
        return reader.GetValue(ordinal) switch
        {
            byte[] bytes => bytes,
            ReadOnlyMemory<byte> memory => memory.ToArray(),
            _ => throw new InvalidDataException("DBObject.jsonbytes is not a BLOB")
        };
    }

    private static byte[] Xor(ReadOnlySpan<byte> bytes)
    {
        var result = bytes.ToArray();
        for (var index = 0; index < result.Length; index++)
            result[index] ^= XorKey;
        return result;
    }

    private static byte[] DecodeBytes(byte[] encoded) => Xor(encoded);
}
