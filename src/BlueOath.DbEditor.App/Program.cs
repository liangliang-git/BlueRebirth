using System.Windows.Forms;

namespace BlueOath.DbEditor.App;

internal static class Program
{
    private const string DefaultConfigRoot =
        @"C:\Users\zhanl\Desktop\BlueOath Rebirth\blueoath\blueoath_Data\StreamingAssets\config";

    [STAThread]
    private static void Main(string[] args)
    {
        ApplicationConfiguration.Initialize();
        var configRoot = args.FirstOrDefault(argument => Directory.Exists(argument)) ?? DefaultConfigRoot;
        Application.Run(new MainForm(configRoot));
    }
}
