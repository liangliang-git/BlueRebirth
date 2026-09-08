using Microsoft.Data.Sqlite;
using Xunit;

namespace BlueOath.DbEditor.Tests;

public sealed class DbArchiveTests
{
    [Fact]
    public void UnpackAndRepack_PreservesJsonAndRawMetadata()
    {
        using var fixture = TestDatabase.Create();
        var archiveRoot = Path.Combine(fixture.Root, "unpacked");
        var rebuiltRoot = Path.Combine(fixture.Root, "rebuilt");

        var archiveFiles = DbArchive.Unpack([fixture.Path], archiveRoot);
        var archiveJson = File.ReadAllText(archiveFiles[0]);
        DbArchive.Repack(archiveRoot, rebuiltRoot);

        var rebuilt = Path.Combine(rebuiltRoot, "config_test.db");
        var document = DbFileEditor.Load(rebuilt);
        Assert.Contains("蒼き鋼", archiveJson, StringComparison.Ordinal);
        Assert.Equal("{\"name\":\"蒼き鋼\",\"value\":42}", document.Records[0].JsonText);
        Assert.Equal("\0", document.Records[1].JsonText);
    }

    [Fact]
    public void Repack_OverwritesExistingDatabaseWithBackup()
    {
        using var fixture = TestDatabase.Create();
        var archiveRoot = Path.Combine(fixture.Root, "unpacked");
        var rebuiltRoot = Path.Combine(fixture.Root, "rebuilt");
        DbArchive.Unpack([fixture.Path], archiveRoot);
        Directory.CreateDirectory(rebuiltRoot);
        var existing = Path.Combine(rebuiltRoot, "config_test.db");
        File.Copy(fixture.Path, existing);

        var result = DbArchive.Repack(archiveRoot, rebuiltRoot);

        Assert.Single(result.Backups);
        Assert.True(File.Exists(result.Backups[0]));
        Assert.True(File.Exists(existing));
    }

    private sealed class TestDatabase : IDisposable
    {
        private TestDatabase(string root, string path)
        {
            Root = root;
            Path = path;
        }

        public string Root { get; }
        public string Path { get; }

        public static TestDatabase Create()
        {
            var root = System.IO.Path.Combine(System.IO.Path.GetTempPath(), "blueoath-db-archive-" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(root);
            var path = System.IO.Path.Combine(root, "config_test.db");
            using var connection = new SqliteConnection(new SqliteConnectionStringBuilder
            {
                DataSource = path,
                Mode = SqliteOpenMode.ReadWriteCreate,
                Pooling = false
            }.ConnectionString);
            connection.Open();
            using var command = connection.CreateCommand();
            command.CommandText = "CREATE TABLE DBObject(id varchar primary key not null, indexid varchar, jsonbytes blob);" +
                                  "INSERT INTO DBObject(id,indexid,jsonbytes) VALUES($id,$indexid,$json);" +
                                  "INSERT INTO DBObject(id,indexid,jsonbytes) VALUES('nill','',X'00');";
            command.Parameters.AddWithValue("$id", "100");
            command.Parameters.AddWithValue("$indexid", "");
            command.Parameters.AddWithValue("$json", DbFileEditor.EncodeJson("{\"name\":\"蒼き鋼\",\"value\":42}"));
            command.ExecuteNonQuery();
            return new TestDatabase(root, path);
        }

        public void Dispose() => Directory.Delete(Root, recursive: true);
    }
}
