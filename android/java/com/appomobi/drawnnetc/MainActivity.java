package com.appomobi.drawnnetc;

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

    // Event kinds sent to the Rust side (nativeEvent). 0..4 are the VPN states of ConnectionService.
    static final int EVENT_FETCHED = 11;

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
        super.onCreate(savedInstanceState); // loads libdrawnnetc.so
    }

    @Override
    protected void onResume() {
        super.onResume();
        // The service may have changed state while the app was away.
        emit(ConnectionService.state, ConnectionService.message);
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
        runOnUiThread(() -> startService(new Intent(this, ConnectionService.class).setAction(ConnectionService.ACTION_STOP)));
    }

    /** A test request through the running tunnel; the service reports the result as its state. */
    public void vpnTest() {
        runOnUiThread(() -> startService(new Intent(this, ConnectionService.class).setAction(ConnectionService.ACTION_TEST)));
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

    /** Downloads a URL as text on a thread of its own; the answer (or "") comes as EVENT_FETCHED. */
    public void fetchText(final String url) {
        new Thread(() -> {
            String body = "";
            java.net.HttpURLConnection connection = null;
            try {
                connection = (java.net.HttpURLConnection) new java.net.URL(url).openConnection();
                connection.setConnectTimeout(20000);
                connection.setReadTimeout(20000);
                connection.setRequestProperty("User-Agent", "NetConnect");
                connection.setRequestProperty("Accept", "application/vnd.github+json");
                if (connection.getResponseCode() == 200) {
                    // A read loop: InputStream.readAllBytes needs Android 13.
                    try (java.io.InputStream in = connection.getInputStream()) {
                        java.io.ByteArrayOutputStream out = new java.io.ByteArrayOutputStream();
                        byte[] buffer = new byte[16384];
                        for (int n; (n = in.read(buffer)) > 0; ) out.write(buffer, 0, n);
                        body = out.toString("UTF-8");
                    }
                }
            } catch (Throwable ignored) {
            } finally {
                if (connection != null) connection.disconnect();
            }
            emit(EVENT_FETCHED, body);
        }).start();
    }

    /** Opens a URL in the browser. */
    public void openUrl(final String url) {
        runOnUiThread(() -> {
            try {
                startActivity(new Intent(Intent.ACTION_VIEW, Uri.parse(url)));
            } catch (Exception ignored) {
            }
        });
    }

    /** Puts text on the clipboard. */
    public void copyText(final String text) {
        runOnUiThread(() -> {
            ClipboardManager clipboard = (ClipboardManager) getSystemService(CLIPBOARD_SERVICE);
            if (clipboard != null) clipboard.setPrimaryClip(ClipData.newPlainText("link", text));
        });
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode != REQUEST_VPN) return;
        if (resultCode == RESULT_OK && pendingConfig != null) {
            startVpn(pendingConfig, pendingNotification);
        } else {
            // The Rust side shows its own text for "permission". Kept as the service's state too:
            // onResume, which runs right after this, sends that state again.
            ConnectionService.state = ConnectionService.STATE_ERROR;
            ConnectionService.message = "permission";
            emit(ConnectionService.STATE_ERROR, "permission");
        }
        pendingConfig = null;
        pendingNotification = null;
    }

    private void startVpn(String config, String notification) {
        Intent intent = new Intent(this, ConnectionService.class)
                .setAction(ConnectionService.ACTION_START)
                .putExtra(ConnectionService.EXTRA_CONFIG, config)
                .putExtra(ConnectionService.EXTRA_NOTIFICATION, notification);
        if (Build.VERSION.SDK_INT >= 26) startForegroundService(intent);
        else startService(intent);
    }
}
