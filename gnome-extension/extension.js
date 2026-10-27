// Exposes the focused window's application id over D-Bus.
//
// Ulanzi Dial cannot read the focused window on GNOME: Mutter refuses
// org.gnome.Shell.Introspect.GetWindows to normal processes and does not
// implement wlr-foreign-toplevel-management. This extension fills that gap:
// it publishes the app id at /dev/ulanzi/Dial/Focus on the session bus and
// emits Changed every time the focus moves.

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

const BUS_NAME = 'dev.ulanzi.Dial.Focus';
const OBJECT_PATH = '/dev/ulanzi/Dial/Focus';

const IFACE_XML = `
<node>
  <interface name="dev.ulanzi.Dial.Focus">
    <method name="Get">
      <arg type="s" direction="out"/>
    </method>
    <signal name="Changed">
      <arg type="s"/>
    </signal>
  </interface>
</node>`;

export default class UlanziDialFocusExtension extends Extension {
    enable() {
        this._appId = '';

        this._dbus = Gio.DBusExportedObject.wrapJSObject(IFACE_XML, this);
        this._dbus.export(Gio.DBus.session, OBJECT_PATH);

        this._nameId = Gio.bus_own_name(
            Gio.BusType.SESSION,
            BUS_NAME,
            Gio.BusNameOwnerFlags.NONE,
            null,
            null,
            null
        );

        this._focusId = global.display.connect('notify::focus-window', () => {
            this._update();
        });

        this._update();
    }

    disable() {
        if (this._focusId) {
            global.display.disconnect(this._focusId);
            this._focusId = 0;
        }
        if (this._nameId) {
            Gio.bus_unown_name(this._nameId);
            this._nameId = 0;
        }
        if (this._dbus) {
            this._dbus.unexport();
            this._dbus = null;
        }
    }

    Get() {
        return this._appId;
    }

    _update() {
        const id = this._currentAppId();
        if (id === this._appId)
            return;

        this._appId = id;
        this._dbus.emit_signal('Changed', new GLib.Variant('(s)', [id]));
    }

    _currentAppId() {
        const win = global.display.focus_window;
        if (!win)
            return '';

        const candidates = [
            win.get_gtk_application_id?.(),
            win.get_sandboxed_app_id?.(),
            win.get_wm_class_instance?.(),
            win.get_wm_class?.(),
        ];

        for (const candidate of candidates) {
            if (candidate)
                return candidate.toLowerCase();
        }
        return '';
    }
}
