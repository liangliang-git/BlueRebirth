using Microsoft.Data.Sqlite;
using Xunit;

namespace BlueOath.DbEditor.Tests;

public sealed class DbFileEditorTests
{
    [Fact]
    public void EncodeDecode_PreservesJapaneseUtf8Json()
    {
        const string json = "{\"name\":\"蒼き鋼\",\"value\":42,\"enabled\":true}";

        var encoded = DbFileEditor.EncodeJson(json);
        var decoded = DbFileEditor.DecodeJson(encoded);

        Assert.NotEqual(System.Text.Encoding.UTF8.GetBytes(json), encoded);
        Assert.Equal(json, decoded);
    }

    [Fact]
    public void Load_ExcludesNoRowsAndMarksMetadata()
    {
        using var fixture = TestDatabase.Create();

        var document = DbFileEditor.Load(fixture.Path);

        Assert.Equal(2, document.Records.Count);
        Assert.Equal("100", document.Records[0].Id);
        Assert.True(document.Records[0].IsEditable);
        Assert.Equal("nill", document.Records[1].Id);
        Assert.False(document.Records[1].IsEditable);
    }

    [Fact]
    public void Save_UpdatesRowAndCreatesTimestampedBackup()
    {
        using var fixture = TestDatabase.Create();
        var backup = DbFileEditor.Save(fixture.Path, "100", "", "{\"value\":99}");

        Assert.True(File.Exists(backup));
        var loaded = DbFileEditor.Load(fixture.Path);
        Assert.Equal("{\"value\":99}", loaded.Records[0].JsonText);
        Assert.Equal("{\"value\":1}", DbFileEditor.Load(backup).Records[0].JsonText);
    }

    [Fact]
    public void EnumerateDatabases_OnlyReturnsConfigDbFilesWithDBObject()
    {
        using var fixture = TestDatabase.Create();
        File.WriteAllText(Path.Combine(fixture.Root, "notes.db"), "not a config");
        File.WriteAllText(Path.Combine(fixture.Root, "config_empty.db"), "not sqlite");

        var paths = DbFileEditor.EnumerateDatabases(fixture.Root);

        Assert.Single(paths);
        Assert.Equal(fixture.Path, paths[0]);
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
            var root = System.IO.Path.Combine(System.IO.Path.GetTempPath(), "blueoath-db-editor-" + Guid.NewGuid().ToString("N"));
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
            command.Parameters.AddWithValue("$json", DbFileEditor.EncodeJson("{\"value\":1}"));
            command.ExecuteNonQuery();
            return new TestDatabase(root, path);
        }

        public void Dispose()
        {
            Directory.Delete(Root, recursive: true);
        }
    }
}
