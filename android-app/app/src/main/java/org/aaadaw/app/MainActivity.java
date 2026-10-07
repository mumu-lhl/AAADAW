package org.aaadaw.app;

import android.app.NativeActivity;
import android.Manifest;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.os.Build;
import android.net.Uri;
import android.os.Bundle;

import java.io.FileInputStream;
import java.io.InputStream;
import java.io.OutputStream;

public final class MainActivity extends NativeActivity {
    private static final int MICROPHONE_REQUEST = 7319;
    private static final int BLUETOOTH_MIDI_REQUEST = 7320;
    private AndroidMidiBridge midiBridge;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        midiBridge = new AndroidMidiBridge(this);
        midiBridge.start();
    }

    @Override
    protected void onDestroy() {
        if (midiBridge != null) {
            midiBridge.close();
            midiBridge = null;
        }
        super.onDestroy();
    }

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
        } else if (requestCode == BLUETOOTH_MIDI_REQUEST) {
            if (midiBridge != null) {
                midiBridge.start();
            }
        }
    }

    private native void nativeMicrophonePermissionResult(boolean granted);

    public void refreshAndroidMidi() {
        if (Build.VERSION.SDK_INT >= 31
                && checkSelfPermission(Manifest.permission.BLUETOOTH_CONNECT)
                        != PackageManager.PERMISSION_GRANTED) {
            runOnUiThread(() -> requestPermissions(
                    new String[] {Manifest.permission.BLUETOOTH_CONNECT},
                    BLUETOOTH_MIDI_REQUEST));
            return;
        }
        if (midiBridge != null) {
            midiBridge.start();
        }
    }

    public String androidMidiPortSummary() {
        int[] counts = midiBridge == null ? new int[] {0, 0} : midiBridge.portCounts();
        return counts[0] + "," + counts[1];
    }

    public byte[] drainAndroidMidiInput() {
        return midiBridge == null ? new byte[0] : midiBridge.drainInputPackets();
    }

    public long androidMidiDroppedInputPackets() {
        return midiBridge == null ? 0 : midiBridge.droppedInputPackets();
    }

    public void sendAndroidMidi(byte[] bytes, long delayNanos) {
        if (midiBridge != null) {
            midiBridge.send(bytes, System.nanoTime() + Math.max(0, delayNanos));
        }
    }

    public void setRecordingServiceEnabled(boolean enabled) {
        setAudioServiceMode(AudioService.ACTION_RECORDING, enabled);
    }

    public void setPlaybackServiceEnabled(boolean enabled) {
        setAudioServiceMode(AudioService.ACTION_PLAYBACK, enabled);
    }

    private void setAudioServiceMode(String mode, boolean enabled) {
        Intent intent = new Intent(this, AudioService.class);
        intent.setAction(enabled ? mode : mode + "_STOP");
        if (enabled && Build.VERSION.SDK_INT >= 26) {
            startForegroundService(intent);
        } else {
            startService(intent);
        }
    }

    public void startRecordingService() {
        setRecordingServiceEnabled(true);
    }

    public void stopRecordingService() {
        setRecordingServiceEnabled(false);
    }

    public void startPlaybackService() {
        setPlaybackServiceEnabled(true);
    }

    public void stopPlaybackService() {
        setPlaybackServiceEnabled(false);
    }
}
