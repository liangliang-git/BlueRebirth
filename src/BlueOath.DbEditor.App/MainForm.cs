using BlueOath.DbEditor;
using System.Drawing;
using System.Text;

namespace BlueOath.DbEditor.App;

internal sealed class MainForm : Form
{
    private readonly TextBox _rootText = new();
    private readonly TextBox _searchText = new();
    private readonly ListBox _databaseList = new();
    private readonly DataGridView _recordGrid = new();
    private readonly TextBox _jsonText = new();
    private readonly Button _saveButton = new();
    private readonly Button _formatButton = new();
    private readonly Button _newButton = new();
    private readonly Button _deleteButton = new();
    private readonly Label _statusLabel = new();
    private readonly Label _recordLabel = new();

    private string _configRoot;
    private IReadOnlyList<string> _databasePaths = [];
    private DbDocument? _document;
    private DbRecord? _currentRecord;
    private bool _loadingEditor;
    private bool _editorDirty;

    public MainForm(string configRoot)
    {
        _configRoot = Path.GetFullPath(configRoot);
        Text = "BlueOath 日服客户端 DB 编辑器";
        StartPosition = FormStartPosition.CenterScreen;
        MinimumSize = new Size(1100, 700);
        Width = 1500;
        Height = 900;

        BuildLayout();
        LoadDatabaseList();
    }

    private void BuildLayout()
    {
        var rootPanel = new TableLayoutPanel
        {
            Dock = DockStyle.Fill,
            ColumnCount = 1,
            RowCount = 3,
            Padding = new Padding(8)
        };
        rootPanel.RowStyles.Add(new RowStyle(SizeType.Absolute, 38));
        rootPanel.RowStyles.Add(new RowStyle(SizeType.Absolute, 34));
        rootPanel.RowStyles.Add(new RowStyle(SizeType.Percent, 100));
        Controls.Add(rootPanel);

        var pathPanel = new TableLayoutPanel { Dock = DockStyle.Fill, ColumnCount = 5 };
        pathPanel.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 72));
        pathPanel.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
        pathPanel.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 88));
        pathPanel.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 100));
        pathPanel.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 100));
        pathPanel.Controls.Add(new Label { Text = "配置目录", Anchor = AnchorStyles.Left, AutoSize = true }, 0, 0);
        _rootText.ReadOnly = true;
        _rootText.Dock = DockStyle.Fill;
        pathPanel.Controls.Add(_rootText, 1, 0);
        var browseButton = new Button { Text = "选择目录", Dock = DockStyle.Fill };
        browseButton.Click += (_, _) => ChooseConfigRoot();
        pathPanel.Controls.Add(browseButton, 2, 0);
        var unpackButton = new Button { Text = "解压选中/批量", Dock = DockStyle.Fill };
        unpackButton.Click += (_, _) => UnpackSelectedDatabases();
        pathPanel.Controls.Add(unpackButton, 3, 0);
        var repackButton = new Button { Text = "重新编译", Dock = DockStyle.Fill };
        repackButton.Click += (_, _) => RepackArchives();
        pathPanel.Controls.Add(repackButton, 4, 0);
        rootPanel.Controls.Add(pathPanel, 0, 0);

        var searchPanel = new TableLayoutPanel { Dock = DockStyle.Fill, ColumnCount = 2 };
        searchPanel.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 72));
        searchPanel.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
        searchPanel.Controls.Add(new Label { Text = "搜索", Anchor = AnchorStyles.Left, AutoSize = true }, 0, 0);
        _searchText.PlaceholderText = "表名、ID、IndexID 或 JSON 内容";
        _searchText.Dock = DockStyle.Fill;
        _searchText.TextChanged += (_, _) => RefreshRecordGrid();
        searchPanel.Controls.Add(_searchText, 1, 0);
        rootPanel.Controls.Add(searchPanel, 0, 1);

        var split = new SplitContainer
        {
            Dock = DockStyle.Fill,
            Orientation = Orientation.Vertical,
            SplitterDistance = 320,
            FixedPanel = FixedPanel.Panel1
        };
        rootPanel.Controls.Add(split, 0, 2);

        _databaseList.Dock = DockStyle.Fill;
        _databaseList.Font = new Font(Font.FontFamily, 10);
        _databaseList.SelectionMode = SelectionMode.MultiExtended;
        _databaseList.HorizontalScrollbar = true;
        _databaseList.SelectedIndexChanged += (_, _) => LoadSelectedDatabase();
        split.Panel1.Controls.Add(_databaseList);

        var editorLayout = new TableLayoutPanel
        {
            Dock = DockStyle.Fill,
            ColumnCount = 1,
            RowCount = 3,
            Padding = new Padding(8, 0, 0, 0)
        };
        editorLayout.RowStyles.Add(new RowStyle(SizeType.Absolute, 36));
        editorLayout.RowStyles.Add(new RowStyle(SizeType.Percent, 46));
        editorLayout.RowStyles.Add(new RowStyle(SizeType.Percent, 54));
        split.Panel2.Controls.Add(editorLayout);

        var actionPanel = new TableLayoutPanel { Dock = DockStyle.Fill, ColumnCount = 6 };
        actionPanel.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 72));
        actionPanel.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 84));
        actionPanel.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 84));
        actionPanel.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 84));
        actionPanel.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 84));
        actionPanel.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
        _recordLabel.Text = "未选择行";
        _recordLabel.Anchor = AnchorStyles.Left;
        actionPanel.Controls.Add(_recordLabel, 0, 0);
        ConfigureButton(actionPanel, _saveButton, "保存", SaveCurrentRecord, 1);
        ConfigureButton(actionPanel, _formatButton, "格式化", FormatCurrentJson, 2);
        ConfigureButton(actionPanel, _newButton, "新增", AddRecord, 3);
        ConfigureButton(actionPanel, _deleteButton, "删除", DeleteCurrentRecord, 4);
        _statusLabel.Text = "就绪";
        _statusLabel.Anchor = AnchorStyles.Left;
        _statusLabel.AutoEllipsis = true;
        actionPanel.Controls.Add(_statusLabel, 5, 0);
        editorLayout.Controls.Add(actionPanel, 0, 0);

        ConfigureGrid();
        editorLayout.Controls.Add(_recordGrid, 0, 1);

        _jsonText.Dock = DockStyle.Fill;
        _jsonText.Multiline = true;
        _jsonText.ScrollBars = ScrollBars.Both;
        _jsonText.AcceptsTab = true;
        _jsonText.WordWrap = false;
        _jsonText.Font = new Font(FontFamily.GenericMonospace, 10);
        _jsonText.TextChanged += (_, _) =>
        {
            if (!_loadingEditor) SetDirty(true);
        };
        editorLayout.Controls.Add(_jsonText, 0, 2);

        SetDirty(false);
    }

    private static void ConfigureButton(TableLayoutPanel panel, Button button, string text, EventHandler handler, int column)
    {
        button.Text = text;
        button.Dock = DockStyle.Fill;
        button.Click += handler;
        panel.Controls.Add(button, column, 0);
    }

    private void ConfigureGrid()
    {
        _recordGrid.Dock = DockStyle.Fill;
        _recordGrid.ReadOnly = true;
        _recordGrid.AllowUserToAddRows = false;
        _recordGrid.AllowUserToDeleteRows = false;
        _recordGrid.AllowUserToResizeRows = false;
        _recordGrid.MultiSelect = false;
        _recordGrid.SelectionMode = DataGridViewSelectionMode.FullRowSelect;
        _recordGrid.AutoGenerateColumns = false;
        _recordGrid.RowHeadersVisible = false;
        _recordGrid.Columns.Add(new DataGridViewTextBoxColumn { HeaderText = "ID", Width = 150, DataPropertyName = "Id" });
        _recordGrid.Columns.Add(new DataGridViewTextBoxColumn { HeaderText = "IndexID", Width = 150, DataPropertyName = "IndexId" });
        _recordGrid.Columns.Add(new DataGridViewTextBoxColumn { HeaderText = "状态", Width = 100, DataPropertyName = "Status" });
        _recordGrid.Columns.Add(new DataGridViewTextBoxColumn { HeaderText = "字节", Width = 80, DataPropertyName = "Size" });
        _recordGrid.Columns.Add(new DataGridViewTextBoxColumn { HeaderText = "错误", AutoSizeMode = DataGridViewAutoSizeColumnMode.Fill, DataPropertyName = "Error" });
        _recordGrid.SelectionChanged += (_, _) => ShowSelectedRecord();
    }

    private void ChooseConfigRoot()
    {
        using var dialog = new FolderBrowserDialog
        {
            Description = "选择日服客户端的 StreamingAssets\\config 目录",
            SelectedPath = Directory.Exists(_configRoot) ? _configRoot : string.Empty,
            UseDescriptionForTitle = true
        };
        if (dialog.ShowDialog(this) != DialogResult.OK) return;
        _configRoot = dialog.SelectedPath;
        LoadDatabaseList();
    }

    private void UnpackSelectedDatabases()
    {
        var selectedPaths = _databaseList.SelectedIndices.Cast<int>()
            .Where(index => index >= 0 && index < _databasePaths.Count)
            .Select(index => _databasePaths[index])
            .ToArray();
        if (selectedPaths.Length == 0)
        {
            MessageBox.Show(this, "先在左侧选择一个或多个 DB 文件。", "没有选择", MessageBoxButtons.OK, MessageBoxIcon.Information);
            return;
        }

        using var dialog = new FolderBrowserDialog
        {
            Description = $"选择解压输出目录（{selectedPaths.Length} 个 DB）",
            UseDescriptionForTitle = true
        };
        if (dialog.ShowDialog(this) != DialogResult.OK) return;
        try
        {
            var files = DbArchive.Unpack(selectedPaths, dialog.SelectedPath);
            SetStatus($"已解压 {files.Count} 个 DB 到 {dialog.SelectedPath}");
        }
        catch (Exception exception)
        {
            ShowError(exception);
        }
    }

    private void RepackArchives()
    {
        using var sourceDialog = new FolderBrowserDialog
        {
            Description = "选择包含 config_*.json 的解压目录",
            UseDescriptionForTitle = true
        };
        if (sourceDialog.ShowDialog(this) != DialogResult.OK) return;

        using var targetDialog = new FolderBrowserDialog
        {
            Description = "选择重新编译后的 DB 输出目录",
            SelectedPath = Directory.Exists(_configRoot) ? _configRoot : string.Empty,
            UseDescriptionForTitle = true
        };
        if (targetDialog.ShowDialog(this) != DialogResult.OK) return;
        try
        {
            var result = DbArchive.Repack(sourceDialog.SelectedPath, targetDialog.SelectedPath);
            SetStatus($"已重新编译 {result.Files.Count} 个 DB，备份 {result.Backups.Count} 个");
            if (string.Equals(Path.GetFullPath(targetDialog.SelectedPath), Path.GetFullPath(_configRoot), StringComparison.OrdinalIgnoreCase))
                LoadDatabaseList();
        }
        catch (Exception exception)
        {
            ShowError(exception);
        }
    }

    private void LoadDatabaseList()
    {
        _rootText.Text = _configRoot;
        try
        {
            _databasePaths = DbFileEditor.EnumerateDatabases(_configRoot);
            _databaseList.Items.Clear();
            foreach (var path in _databasePaths)
                _databaseList.Items.Add(Path.GetFileName(path));
            SetStatus($"发现 {_databasePaths.Count} 个 DBObject 配置表");
            if (_databaseList.Items.Count > 0) _databaseList.SelectedIndex = 0;
            else ClearEditor("目录中没有可编辑的 config_*.db");
        }
        catch (Exception exception)
        {
            ClearEditor(exception.Message);
            ShowError(exception);
        }
    }

    private void LoadSelectedDatabase()
    {
        if (_databaseList.SelectedIndex < 0 || _databaseList.SelectedIndex >= _databasePaths.Count) return;
        if (!TryDiscardChanges()) return;
        try
        {
            var path = _databasePaths[_databaseList.SelectedIndex];
            _document = DbFileEditor.Load(path);
            _currentRecord = null;
            RefreshRecordGrid();
            SetStatus($"{Path.GetFileName(path)}：{_document.EditableCount}/{_document.Records.Count} 行可编辑");
        }
        catch (Exception exception)
        {
            ClearEditor(exception.Message);
            ShowError(exception);
        }
    }

    private void RefreshRecordGrid()
    {
        _recordGrid.Rows.Clear();
        if (_document is null) return;
        var query = _searchText.Text.Trim();
        foreach (var record in _document.Records)
        {
            if (!Matches(record, query)) continue;
            var row = _recordGrid.Rows.Add(
                record.Id,
                record.IndexId ?? string.Empty,
                record.IsEditable ? "可编辑" : "只读",
                record.EncodedSize,
                record.Error ?? string.Empty);
            _recordGrid.Rows[row].Tag = record;
            if (!record.IsEditable)
                _recordGrid.Rows[row].DefaultCellStyle.ForeColor = Color.Gray;
        }
        if (_recordGrid.Rows.Count > 0)
            _recordGrid.Rows[0].Selected = true;
        else
            ClearEditor("没有匹配行");
    }

    private static bool Matches(DbRecord record, string query)
    {
        if (query.Length == 0) return true;
        return record.Id.Contains(query, StringComparison.OrdinalIgnoreCase) ||
               (record.IndexId?.Contains(query, StringComparison.OrdinalIgnoreCase) ?? false) ||
               record.JsonText.Contains(query, StringComparison.OrdinalIgnoreCase);
    }

    private void ShowSelectedRecord()
    {
        if (_recordGrid.CurrentRow?.Tag is not DbRecord record) return;
        if (_editorDirty && !TryDiscardChanges()) return;
        _currentRecord = record;
        _loadingEditor = true;
        try
        {
            _jsonText.Text = record.IsEditable ? DbFileEditor.FormatJson(record.JsonText) : record.JsonText;
            _recordLabel.Text = $"ID {record.Id}";
            _jsonText.ReadOnly = !record.IsEditable;
            SetDirty(false);
        }
        catch (Exception exception)
        {
            ClearEditor(exception.Message);
            ShowError(exception);
        }
        finally
        {
            _loadingEditor = false;
        }
    }

    private void SaveCurrentRecord(object? sender, EventArgs e)
    {
        if (_document is null || _currentRecord is null || !_currentRecord.IsEditable) return;
        try
        {
            var backup = DbFileEditor.Save(_document.Path, _currentRecord.Id, _currentRecord.IndexId, _jsonText.Text);
            ReloadCurrentDatabase(_currentRecord.Id, _currentRecord.IndexId);
            SetStatus($"已保存。备份：{Path.GetFileName(backup)}");
        }
        catch (Exception exception)
        {
            ShowError(exception);
        }
    }

    private void FormatCurrentJson(object? sender, EventArgs e)
    {
        if (_jsonText.ReadOnly) return;
        try
        {
            _loadingEditor = true;
            _jsonText.Text = DbFileEditor.FormatJson(_jsonText.Text);
            SetDirty(true);
        }
        catch (Exception exception)
        {
            ShowError(exception);
        }
        finally
        {
            _loadingEditor = false;
        }
    }

    private void AddRecord(object? sender, EventArgs e)
    {
        if (_document is null) return;
        if (!TryDiscardChanges()) return;
        if (!RowPromptDialog.TryShow(this, out var id, out var indexId)) return;
        try
        {
            var backup = DbFileEditor.Add(_document.Path, id, indexId.Length == 0 ? null : indexId, "{}");
            ReloadCurrentDatabase(id, indexId.Length == 0 ? null : indexId);
            SetStatus($"已新增 {id}。备份：{Path.GetFileName(backup)}");
        }
        catch (Exception exception)
        {
            ShowError(exception);
        }
    }

    private void DeleteCurrentRecord(object? sender, EventArgs e)
    {
        if (_document is null || _currentRecord is null || !_currentRecord.IsEditable) return;
        if (MessageBox.Show(this, $"确定删除 ID={_currentRecord.Id}？删除前会自动备份。", "确认删除",
                MessageBoxButtons.YesNo, MessageBoxIcon.Warning) != DialogResult.Yes) return;
        try
        {
            var backup = DbFileEditor.Delete(_document.Path, _currentRecord.Id);
            ReloadCurrentDatabase(null, null);
            SetStatus($"已删除。备份：{Path.GetFileName(backup)}");
        }
        catch (Exception exception)
        {
            ShowError(exception);
        }
    }

    private void ReloadCurrentDatabase(string? id, string? indexId)
    {
        if (_document is null) return;
        _document = DbFileEditor.Load(_document.Path);
        _currentRecord = null;
        RefreshRecordGrid();
        if (id is null) return;
        foreach (DataGridViewRow row in _recordGrid.Rows)
        {
            if (row.Tag is DbRecord record && record.Id == id && record.IndexId == indexId)
            {
                row.Selected = true;
                _recordGrid.CurrentCell = row.Cells[0];
                break;
            }
        }
    }

    private bool TryDiscardChanges()
    {
        if (!_editorDirty) return true;
        var result = MessageBox.Show(this, "当前 JSON 有未保存修改，放弃？", "未保存修改",
            MessageBoxButtons.YesNo, MessageBoxIcon.Warning);
        if (result != DialogResult.Yes) return false;
        SetDirty(false);
        return true;
    }

    private void SetDirty(bool dirty)
    {
        _editorDirty = dirty;
        _saveButton.Enabled = dirty && _currentRecord?.IsEditable == true;
        _formatButton.Enabled = _currentRecord?.IsEditable == true;
        _newButton.Enabled = _document is not null;
        _deleteButton.Enabled = _currentRecord?.IsEditable == true;
        _recordLabel.Text = _currentRecord is null ? "未选择行" : $"ID {_currentRecord.Id}{(dirty ? " *" : string.Empty)}";
    }

    private void ClearEditor(string message)
    {
        _document = null;
        _currentRecord = null;
        _recordGrid.Rows.Clear();
        _loadingEditor = true;
        _jsonText.Clear();
        _jsonText.ReadOnly = true;
        _loadingEditor = false;
        SetDirty(false);
        SetStatus(message);
    }

    private void SetStatus(string message) => _statusLabel.Text = message;

    private void ShowError(Exception exception) =>
        MessageBox.Show(this, exception.Message, "操作失败", MessageBoxButtons.OK, MessageBoxIcon.Error);

    protected override void OnFormClosing(FormClosingEventArgs e)
    {
        if (!TryDiscardChanges())
        {
            e.Cancel = true;
            return;
        }
        base.OnFormClosing(e);
    }
}

internal sealed class RowPromptDialog : Form
{
    private readonly TextBox _idText = new();
    private readonly TextBox _indexText = new();

    private RowPromptDialog()
    {
        Text = "新增 DB 行";
        FormBorderStyle = FormBorderStyle.FixedDialog;
        StartPosition = FormStartPosition.CenterParent;
        MinimizeBox = false;
        MaximizeBox = false;
        ClientSize = new Size(360, 150);

        var layout = new TableLayoutPanel { Dock = DockStyle.Fill, Padding = new Padding(12), RowCount = 3, ColumnCount = 2 };
        layout.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 72));
        layout.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
        layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 32));
        layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 32));
        layout.RowStyles.Add(new RowStyle(SizeType.Percent, 100));
        layout.Controls.Add(new Label { Text = "ID", Anchor = AnchorStyles.Left, AutoSize = true }, 0, 0);
        _idText.Dock = DockStyle.Fill;
        layout.Controls.Add(_idText, 1, 0);
        layout.Controls.Add(new Label { Text = "IndexID", Anchor = AnchorStyles.Left, AutoSize = true }, 0, 1);
        _indexText.Dock = DockStyle.Fill;
        layout.Controls.Add(_indexText, 1, 1);
        var buttons = new FlowLayoutPanel { FlowDirection = FlowDirection.RightToLeft, Dock = DockStyle.Fill };
        var ok = new Button { Text = "确定", DialogResult = DialogResult.OK, Width = 72 };
        var cancel = new Button { Text = "取消", DialogResult = DialogResult.Cancel, Width = 72 };
        buttons.Controls.Add(ok);
        buttons.Controls.Add(cancel);
        layout.Controls.Add(buttons, 1, 2);
        Controls.Add(layout);
        AcceptButton = ok;
        CancelButton = cancel;
    }

    public static bool TryShow(IWin32Window owner, out string id, out string indexId)
    {
        using var dialog = new RowPromptDialog();
        var result = dialog.ShowDialog(owner);
        id = dialog._idText.Text.Trim();
        indexId = dialog._indexText.Text.Trim();
        if (result != DialogResult.OK || id.Length == 0)
        {
            id = string.Empty;
            indexId = string.Empty;
            return false;
        }
        return true;
    }
}
