package org.aaadaw.app;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Intent;
import android.content.pm.ServiceInfo;
import android.os.Build;
import android.os.IBinder;

public final class AudioService extends Service {
    public static final String ACTION_RECORDING = "org.aaadaw.app.action.RECORDING";
    public static final String ACTION_PLAYBACK = "org.aaadaw.app.action.PLAYBACK";
    private static final String CHANNEL_ID = "aaadaw-audio";
    private static final int NOTIFICATION_ID = 1;

    private boolean recording;
    private boolean playback;

    @Override
    public void onCreate() {
        super.onCreate();
        if (Build.VERSION.SDK_INT >= 26) {
            NotificationManager manager = getSystemService(NotificationManager.class);
            manager.createNotificationChannel(new NotificationChannel(
                    CHANNEL_ID,
                    "AAADAW audio",
                    NotificationManager.IMPORTANCE_LOW));
        }
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        String action = intent == null ? null : intent.getAction();
        if (ACTION_RECORDING.equals(action)) {
            recording = true;
        } else if ((ACTION_RECORDING + "_STOP").equals(action)) {
            recording = false;
        } else if (ACTION_PLAYBACK.equals(action)) {
            playback = true;
        } else if ((ACTION_PLAYBACK + "_STOP").equals(action)) {
            playback = false;
        }

        if (!recording && !playback) {
            stopForeground(STOP_FOREGROUND_REMOVE);
            stopSelf(startId);
            return START_NOT_STICKY;
        }

        Notification notification = notification();
        if (Build.VERSION.SDK_INT >= 29) {
            startForeground(NOTIFICATION_ID, notification, foregroundServiceTypes());
        } else {
            startForeground(NOTIFICATION_ID, notification);
        }
        return START_NOT_STICKY;
    }

    private Notification notification() {
        Intent openApp = new Intent(this, MainActivity.class);
        PendingIntent pendingIntent = PendingIntent.getActivity(
                this,
                0,
                openApp,
                PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
        String title = recording && playback
                ? "AAADAW is recording and playing"
                : recording ? "AAADAW is recording" : "AAADAW is playing";
        Notification.Builder builder = Build.VERSION.SDK_INT >= 26
                ? new Notification.Builder(this, CHANNEL_ID)
                : new Notification.Builder(this);
        return builder
                .setContentTitle(title)
                .setContentText("Tap to return to AAADAW")
                .setSmallIcon(android.R.drawable.ic_btn_speak_now)
                .setContentIntent(pendingIntent)
                .setOngoing(true)
                .build();
    }

    private int foregroundServiceTypes() {
        if (Build.VERSION.SDK_INT < 29) {
            return 0;
        }
        int types = 0;
        if (recording) {
            types |= ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE;
        }
        if (playback) {
            types |= ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK;
        }
        return types;
    }

    @Override
    public IBinder onBind(Intent intent) {
        return null;
    }
}
