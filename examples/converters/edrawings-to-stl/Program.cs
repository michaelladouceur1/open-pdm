// edrawings-to-stl: a reference OpenPDM external converter.
//
// Converts a file OpenPDM cannot parse itself (SolidWorks, Creo/Pro-E, …) into
// STL using the FREE eDrawings Viewer API — no SOLIDWORKS license required.
//
// OpenPDM invokes this as:  edrawings-to-stl <input> <output.stl>
// It must write STL to <output>, exit 0 on success, and exit non-zero (with a
// message on stderr) on failure — matching OpenPDM's converter contract.
//
// The eDrawings control is hosted in a hidden WinForms window because its
// OpenDoc/Save calls are asynchronous and report completion via events.
using System;
using System.IO;
using System.Windows.Forms;
using eDrawings.Interop.EModelViewControl;

namespace EDrawingsToStl
{
    internal static class Program
    {
        private static EModelViewControl? _ctrl;
        private static string _input = "";
        private static string _output = "";
        private static int _exitCode = 1;

        [STAThread]
        private static int Main(string[] args)
        {
            if (args.Length < 2)
            {
                Console.Error.WriteLine("usage: edrawings-to-stl <input> <output.stl>");
                return 2;
            }

            _input = Path.GetFullPath(args[0]);
            _output = Path.ChangeExtension(Path.GetFullPath(args[1]), ".stl");

            var outputDir = Path.GetDirectoryName(_output);
            if (!string.IsNullOrEmpty(outputDir))
            {
                Directory.CreateDirectory(outputDir);
            }

            var host = new EDrawingsHost();
            host.ControlLoaded += OnControlLoaded;

            // eDrawings needs a message pump; keep its window out of the way.
            var form = new Form
            {
                ShowIcon = false,
                ShowInTaskbar = false,
                WindowState = FormWindowState.Minimized,
            };
            form.Controls.Add(host);
            host.Dock = DockStyle.Fill;

            Application.Run(form);
            return _exitCode;
        }

        private static void OnControlLoaded(EModelViewControl ctrl)
        {
            _ctrl = ctrl;

            ctrl.OnFinishedLoadingDocument += OnDocumentLoaded;
            ctrl.OnFailedLoadingDocument += (file, code, message) =>
                Finish($"failed to open {file}: {message} ({code})", 1);

            ctrl.OnFinishedSavingDocument += () => Finish(null, 0);
            ctrl.OnFailedSavingDocument += (file, code, message) =>
                Finish($"failed to save {file}: {message} ({code})", 1);

            ctrl.OpenDoc(_input, false, false, false, "");
        }

        private static void OnDocumentLoaded(string fileName)
        {
            // Save infers the target format from the extension (.stl -> STL export).
            _ctrl!.Save(_output, false, "");
        }

        private static void Finish(string? error, int exitCode)
        {
            if (error != null)
            {
                Console.Error.WriteLine(error);
            }
            _exitCode = exitCode;
            _ctrl?.CloseActiveDoc("");
            Application.ExitThread();
        }
    }
}
