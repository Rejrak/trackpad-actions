import Gio from 'gi://Gio';

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

const SERVICE_NAME = 'io.github.Rejrak.Trackpadd';
const OBJECT_PATH = '/io/github/Rejrak/Trackpadd';
const INTERFACE_NAME = 'io.github.Rejrak.Trackpadd1';
const ACTION_VALUE_CHANGED_SIGNAL = 'ActionValueChanged';

function clamp(value, min, max) {
    return Math.min(max, Math.max(min, value));
}

function volumeIcon(level) {
    if (level <= 0)
        return 'audio-volume-muted-symbolic';
    if (level < 0.34)
        return 'audio-volume-low-symbolic';
    if (level < 0.67)
        return 'audio-volume-medium-symbolic';
    if (level <= 1)
        return 'audio-volume-high-symbolic';
    return 'audio-volume-overamplified-symbolic';
}

function formatDuration(seconds) {
    const total = Math.max(0, Math.round(seconds));
    const hours = Math.floor(total / 3600);
    const minutes = Math.floor((total % 3600) / 60);
    const remainder = total % 60;

    if (hours > 0)
        return `${hours}:${minutes.toString().padStart(2, '0')}:${remainder.toString().padStart(2, '0')}`;

    return `${minutes.toString().padStart(2, '0')}:${remainder.toString().padStart(2, '0')}`;
}

function truncate(text, maximum) {
    if (text.length <= maximum)
        return text;

    return `${text.slice(0, Math.max(1, maximum - 1))}…`;
}

function mediaLabel(source, title, artist, value, maxValue) {
    const position = maxValue > 0
        ? `${formatDuration(value)} / ${formatDuration(maxValue)}`
        : formatDuration(value);

    if (title && artist)
        return `${truncate(artist, 20)} — ${truncate(title, 34)} · ${position}`;

    if (title && source)
        return `${truncate(source, 16)} · ${truncate(title, 36)} · ${position}`;

    if (title)
        return `${truncate(title, 44)} · ${position}`;

    if (artist)
        return `${truncate(artist, 36)} · ${position}`;

    if (source)
        return `${truncate(source, 24)} · ${position}`;

    return `Media · ${position}`;
}

export default class TrackpaddNativeOsdExtension extends Extension {
    enable() {
        this._subscriptionId = Gio.DBus.session.signal_subscribe(
            SERVICE_NAME,
            INTERFACE_NAME,
            ACTION_VALUE_CHANGED_SIGNAL,
            OBJECT_PATH,
            null,
            Gio.DBusSignalFlags.NONE,
            (_connection, _senderName, _objectPath, _interfaceName, _signalName, parameters) => {
                try {
                    const [actionId, kind, value, maxValue, unit, metadata] =
                        parameters.deepUnpack();
                    const [source, title, artist] = metadata;
                    this._onActionValue(
                        actionId,
                        kind,
                        value,
                        maxValue,
                        unit,
                        source,
                        title,
                        artist
                    );
                } catch (error) {
                    console.error(`[trackpadd OSD] failed to handle D-Bus event: ${error}`);
                }
            }
        );
    }

    disable() {
        if (this._subscriptionId) {
            Gio.DBus.session.signal_unsubscribe(this._subscriptionId);
            this._subscriptionId = 0;
        }

        Main.osdWindowManager.hideAll();
    }

    _onActionValue(_actionId, kind, value, maxValue, unit, source, title, artist) {
        if (!Number.isFinite(value) || !Number.isFinite(maxValue))
            return;

        switch (`${kind}:${unit}`) {
        case 'brightness:percent': {
            const maximum = maxValue > 0 ? maxValue : 100;
            this._show(
                'display-brightness-symbolic',
                'Brightness',
                clamp(value / maximum, 0, 1),
                1
            );
            break;
        }

        case 'volume:percent': {
            const rawLevel = Math.max(0, value / 100);
            const maximum = maxValue > 0 ? maxValue : 100;
            const label = source ? truncate(source, 32) : 'Volume';
            this._show(
                volumeIcon(rawLevel),
                label,
                clamp(value / maximum, 0, 1),
                1
            );
            break;
        }

        case 'media-position:seconds': {
            const label = mediaLabel(source, title, artist, value, maxValue);
            this._show(
                'media-playback-start-symbolic',
                label,
                maxValue > 0 ? clamp(value / maxValue, 0, 1) : null,
                1
            );
            break;
        }

        default:
            break;
        }
    }

    _show(iconName, label, level, maxLevel) {
        const icon = Gio.Icon.new_for_string(iconName);

        // GNOME 49+.
        if (typeof Main.osdWindowManager.showAll === 'function') {
            Main.osdWindowManager.showAll(icon, label, level, maxLevel);
            return;
        }

        // GNOME 45–48.
        Main.osdWindowManager.show(-1, icon, label, level, maxLevel);
    }
}
