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
            // Only reached if the form closes without a result (e.g. dismissed).
            // A successful or failed conversion exits directly in Finish().
            return 1;
        }

        private static void OnControlLoaded(EModelViewControl ctrl)
        {
            _ctrl = ctrl;

            // Run unattended: eDrawings otherwise pops a modal "STL Export
            // Options" dialog on Save, which hangs a headless converter until the
            // caller's timeout kills it. Silent mode + no save prompt keep it
            // headless. Must be set before OpenDoc/Save.
            ctrl.EnableFeature[EMVEnableFeatures.eMVSuppressSavePrompt] = true;
            ctrl.EnableFeature[EMVEnableFeatures.eMVEnableSilentMode] = true;

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

            // Exit the process outright rather than Application.ExitThread():
            // unwinding the eDrawings control can block indefinitely even after
            // the output file has been written and closed, which would leave a
            // headless converter (and its caller) hanging until the timeout.
            Environment.Exit(exitCode);
        }
    }
}
