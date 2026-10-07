package org.aaadaw.app;

import android.app.NativeActivity;
import android.Manifest;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.os.Build;
import android.net.Uri;

import java.io.FileInputStream;
import java.io.InputStream;
import java.io.OutputStream;

public final class MainActivity extends NativeActivity {
    private static final int MICROPHONE_REQUEST = 7319;

    public boolean copyLocalFileToUri(String uriValue, String localPath) {
        Uri uri = Uri.parse(uriValue);
        try (InputStream input = new FileInputStream(localPath);
             OutputStream output = getContentResolver().openOutputStream(uri, "wt")) {
            if (output == null) {
                return false;
            }
            byte[] buffer = new byte[64 * 1024];
            int count;
            while ((count = input.read(buffer)) != -1) {
                output.write(buffer, 0, count);
            }
            output.flush();
            return true;
        } catch (Exception error) {
            return false;
        }
    }

    public boolean hasMicrophonePermission() {
        return checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED;
    }

    public void requestMicrophonePermission() {
        runOnUiThread(() -> {
            if (Build.VERSION.SDK_INT >= 33) {
                requestPermissions(new String[] {
                    Manifest.permission.RECORD_AUDIO,
                    Manifest.permission.POST_NOTIFICATIONS
                }, MICROPHONE_REQUEST);
            } else {
                requestPermissions(new String[] { Manifest.permission.RECORD_AUDIO }, MICROPHONE_REQUEST);
            }
        });
    }

    @Override
    public void onRequestPermissionsResult(int requestCode, String[] permissions, int[] grantResults) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults);
        if (requestCode == MICROPHONE_REQUEST) {
            boolean granted = grantResults.length > 0
                    && grantResults[0] == PackageManager.PERMISSION_GRANTED;
            nativeMicrophonePermissionResult(granted);
        }
    }

    private native void nativeMicrophonePermissionResult(boolean granted);

    public void startRecordingService() {
        Intent intent = new Intent(this, RecordingService.class);
        if (Build.VERSION.SDK_INT >= 26) {
            startForegroundService(intent);
        } else {
            startService(intent);
        }
    }

    public void stopRecordingService() {
        stopService(new Intent(this, RecordingService.class));
    }
}
