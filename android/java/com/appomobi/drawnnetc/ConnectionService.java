package com.appomobi.drawnnetc;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.content.Intent;
import android.content.pm.ServiceInfo;
import android.net.VpnService;
import android.os.Build;
import android.os.ParcelFileDescriptor;
import android.util.Log;

import libv2ray.CoreCallbackHandler;
import libv2ray.CoreController;
import libv2ray.Libv2ray;

/**
 * The tunnel: Android's VPN interface with every route, and Xray-core reading it (its own TUN
 * inbound on the interface's fd). The app itself is left out of the VPN, so Xray's connections to
 * the server go out over the real network.
 */
public class ConnectionService extends VpnService implements CoreCallbackHandler {
    static final String ACTION_START = "com.appomobi.drawnnetc.START";
    static final String ACTION_STOP = "com.appomobi.drawnnetc.STOP";
    static final String ACTION_TEST = "com.appomobi.drawnnetc.TEST";
    static final String EXTRA_CONFIG = "config";
    static final String EXTRA_NOTIFICATION = "notification";

    static final int STATE_OFF = 0;
    static final int STATE_CONNECTING = 1;
    static final int STATE_CONNECTED = 2;
    static final int STATE_ERROR = 3;
    /** The tunnel is up but a test request through the server got no answer. */
    static final int STATE_NO_RESPONSE = 4;

    static volatile int state = STATE_OFF;
    static volatile String message = "";

    private static final String TAG = "NetConnect";
    private static final String CHANNEL = "vpn";
    private static final int NOTIFICATION = 1;
    /** Answers 204 with no body: a cheap check that traffic really passes the server. */
    private static final String PROBE_URL = "https://www.gstatic.com/generate_204";

    private final Object lock = new Object();
    private CoreController core;
    private ParcelFileDescriptor tun;

    private static void report(int newState, String text) {
        state = newState;
        message = text == null ? "" : text;
        Log.i(TAG, "state " + newState + " " + message);
        MainActivity.emit(newState, message);
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        if (intent == null || ACTION_STOP.equals(intent.getAction())) {
            new Thread(this::stopVpn).start();
            return START_NOT_STICKY;
        }
        if (ACTION_TEST.equals(intent.getAction())) {
            new Thread(this::probe).start();
            return START_NOT_STICKY;
        }
        final String config = intent.getStringExtra(EXTRA_CONFIG);
        startInForeground(intent.getStringExtra(EXTRA_NOTIFICATION));
        report(STATE_CONNECTING, "Connecting");
        new Thread(() -> startVpn(config)).start();
        return START_NOT_STICKY;
    }

    private void startVpn(String config) {
        synchronized (lock) {
            stopCore();
            try {
                Builder builder = new Builder()
                        .setSession("Net Connect")
                        .setMtu(1500)
                        .addAddress("10.10.14.1", 30)
                        .addRoute("0.0.0.0", 0)
                        .addAddress("fd66:7665:7373::1", 126)
                        .addRoute("::", 0)
                        .addDnsServer("1.1.1.1");
                builder.addDisallowedApplication(getPackageName());
                if (Build.VERSION.SDK_INT >= 29) builder.setMetered(false);
                tun = builder.establish();
                if (tun == null) throw new IllegalStateException("VPN permission was revoked");
                Libv2ray.initCoreEnv(getFilesDir().getAbsolutePath(), "");
                core = Libv2ray.newCoreController(this);
                core.startLoop(config, tun.getFd());
            } catch (Throwable e) {
                Log.e(TAG, "start failed", e);
                stopCore();
                report(STATE_ERROR, e.getMessage() == null ? e.toString() : e.getMessage());
                stopForeground(true);
                stopSelf();
                return;
            }
            probe();
        }
    }

    /** A test request through the server: connected with its delay, or no answer. */
    private void probe() {
        synchronized (lock) {
            if (core == null) return;
            try {
                long ms = core.measureDelay(PROBE_URL);
                report(STATE_CONNECTED, String.valueOf(ms)); // milliseconds; the app adds the unit
            } catch (Throwable e) {
                Log.w(TAG, "probe failed", e);
                report(STATE_NO_RESPONSE, "The server does not answer");
            }
        }
    }

    private void stopVpn() {
        synchronized (lock) {
            stopCore();
            report(STATE_OFF, "");
            stopForeground(true);
            stopSelf();
        }
    }

    private void stopCore() {
        if (core != null) {
            try {
                core.stopLoop();
            } catch (Throwable e) {
                Log.w(TAG, "stop failed", e);
            }
            core = null;
        }
        if (tun != null) {
            try {
                tun.close();
            } catch (Throwable ignored) {
            }
            tun = null;
        }
    }

    /** Another VPN app took over, or the user turned the VPN off in the system settings. */
    @Override
    public void onRevoke() {
        new Thread(this::stopVpn).start();
    }

    @Override
    public void onDestroy() {
        synchronized (lock) {
            stopCore();
        }
        if (state != STATE_ERROR) report(STATE_OFF, "");
        super.onDestroy();
    }

    private void startInForeground(String text) {
        NotificationManager manager = getSystemService(NotificationManager.class);
        if (manager != null && manager.getNotificationChannel(CHANNEL) == null) {
            manager.createNotificationChannel(new NotificationChannel(CHANNEL, "VPN", NotificationManager.IMPORTANCE_LOW));
        }
        PendingIntent open = PendingIntent.getActivity(this, 0, new Intent(this, MainActivity.class),
                PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT);
        Notification notification = new Notification.Builder(this, CHANNEL)
                .setSmallIcon(android.R.drawable.ic_lock_lock)
                .setContentTitle("Net Connect")
                .setContentText(text == null ? "VPN" : text)
                .setContentIntent(open)
                .setOngoing(true)
                .build();
        if (Build.VERSION.SDK_INT >= 34) {
            startForeground(NOTIFICATION, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE);
        } else {
            startForeground(NOTIFICATION, notification);
        }
    }

    // Xray-core's callbacks (gomobile CoreCallbackHandler).
    @Override
    public long startup() {
        return 0;
    }

    @Override
    public long shutdown() {
        return 0;
    }

    @Override
    public long onEmitStatus(long code, String text) {
        Log.i(TAG, "core " + code + " " + text);
        return 0;
    }
}
