package com.appomobi.drawnvless;

import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Intent;
import android.net.Uri;
import android.net.VpnService;
import android.os.Build;
import android.os.Bundle;

import com.google.androidgamesdk.GameActivity;

/** The DrawnUI app's activity. The Rust side calls the public methods through JNI (any thread). */
public class MainActivity extends GameActivity {
    private static final int REQUEST_VPN = 1;

    // Event kinds sent to the Rust side (nativeEvent). 0..4 are the VPN states of XrayVpnService.
    static final int EVENT_LINK = 10;

    private String pendingConfig;
    private String pendingNotification;

    /** Implemented in Rust (src/android.rs); queues the event for the UI thread. */
    private static native void nativeEvent(int kind, String text);

    /** Sends an event to the Rust side; does nothing while the library is not loaded (a service
     *  started without the activity). */
    static void emit(int kind, String text) {
        try {
            nativeEvent(kind, text == null ? "" : text);
        } catch (UnsatisfiedLinkError ignored) {
        }
    }

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState); // loads libdrawnvless.so
        deliverLink(getIntent());
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);
        deliverLink(intent);
    }

    @Override
    protected void onResume() {
        super.onResume();
        // The service may have changed state while the app was away.
        emit(XrayVpnService.state, XrayVpnService.message);
    }

    private void deliverLink(Intent intent) {
        Uri data = intent == null ? null : intent.getData();
        if (data != null && "vless".equalsIgnoreCase(data.getScheme())) {
            emit(EVENT_LINK, data.toString());
            intent.setData(null);
        }
    }

    /** Starts the VPN with an Xray config; asks the system for the VPN permission the first time. */
    public void vpnConnect(final String config, final String notification) {
        runOnUiThread(() -> {
            Intent consent = VpnService.prepare(this);
            if (consent != null) {
                pendingConfig = config;
                pendingNotification = notification;
                startActivityForResult(consent, REQUEST_VPN);
            } else {
                startVpn(config, notification);
            }
        });
    }

    /** The phone's language code ("ru", "en"...). */
    public String systemLanguage() {
        return java.util.Locale.getDefault().getLanguage();
    }

    /** The library and Xray-core versions. */
    public String coreVersion() {
        return libv2ray.Libv2ray.checkVersionX();
    }

    public void vpnDisconnect() {
        runOnUiThread(() -> {
            Intent intent = new Intent(this, XrayVpnService.class).setAction(XrayVpnService.ACTION_STOP);
            startService(intent);
        });
    }

    /** The clipboard's text, or "" (Android lets only the focused app read it). */
    public String clipboardText() {
        ClipboardManager clipboard = (ClipboardManager) getSystemService(CLIPBOARD_SERVICE);
        if (clipboard == null || !clipboard.hasPrimaryClip()) return "";
        ClipData clip = clipboard.getPrimaryClip();
        if (clip == null || clip.getItemCount() == 0) return "";
        CharSequence text = clip.getItemAt(0).coerceToText(this);
        return text == null ? "" : text.toString();
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode != REQUEST_VPN) return;
        if (resultCode == RESULT_OK && pendingConfig != null) {
            startVpn(pendingConfig, pendingNotification);
        } else {
            // The Rust side shows its own text for "permission".
            emit(XrayVpnService.STATE_ERROR, "permission");
        }
        pendingConfig = null;
        pendingNotification = null;
    }

    private void startVpn(String config, String notification) {
        Intent intent = new Intent(this, XrayVpnService.class)
                .setAction(XrayVpnService.ACTION_START)
                .putExtra(XrayVpnService.EXTRA_CONFIG, config)
                .putExtra(XrayVpnService.EXTRA_NOTIFICATION, notification);
        if (Build.VERSION.SDK_INT >= 26) startForegroundService(intent);
        else startService(intent);
    }
}
