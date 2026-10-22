// Thin AxHost wrapper around the eDrawings ActiveX control (IEModelViewControl).
// The GUID is the CLSID that the eDrawings Viewer installer registers.
using System;
using System.Windows.Forms;
using eDrawings.Interop.EModelViewControl;

namespace EDrawingsToStl
{
    public sealed class EDrawingsHost : AxHost
    {
        public event Action<EModelViewControl>? ControlLoaded;
        private bool _loaded;

        public EDrawingsHost()
            : base("22945A69-1191-4DCF-9E6F-409BDE94D101")
        {
        }

        protected override void OnCreateControl()
        {
            base.OnCreateControl();
            if (!_loaded)
            {
                _loaded = true;
                ControlLoaded?.Invoke(GetOcx() as EModelViewControl);
            }
        }
    }
}
