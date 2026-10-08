package org.aaadaw.app;

import android.content.Context;
import android.media.midi.MidiDevice;
import android.media.midi.MidiDeviceInfo;
import android.media.midi.MidiInputPort;
import android.media.midi.MidiManager;
import android.media.midi.MidiOutputPort;
import android.media.midi.MidiReceiver;
import android.os.Handler;
import android.os.Looper;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.atomic.AtomicLong;

/** Android USB and paired Bluetooth MIDI 1.0 device ports. */
final class AndroidMidiBridge implements AutoCloseable {
    private static final int INPUT_QUEUE_CAPACITY = 2048;
    private static final int MAX_DRAINED_PACKETS = 256;
    private static final int MAX_PACKET_BYTES = 4096;

    private final MidiManager manager;
    private final Handler handler = new Handler(Looper.getMainLooper());
    private final Object lock = new Object();
    private final Map<Integer, MidiDevice> devices = new HashMap<>();
    private final Set<Integer> opening = new HashSet<>();
    private final Map<String, MidiOutputPort> inputPorts = new HashMap<>();
    private final Map<String, MidiInputPort> outputPorts = new HashMap<>();
    private final ArrayBlockingQueue<byte[]> inputPackets = new ArrayBlockingQueue<>(INPUT_QUEUE_CAPACITY);
    private final AtomicLong droppedPackets = new AtomicLong();
    private boolean registered;

    private final MidiManager.DeviceCallback callback = new MidiManager.DeviceCallback() {
        @Override
        public void onDeviceAdded(MidiDeviceInfo info) {
            openDevice(info);
        }

        @Override
        public void onDeviceRemoved(MidiDeviceInfo info) {
            closeDevice(info.getId());
        }
    };

    AndroidMidiBridge(Context context) {
        this.manager = context.getSystemService(MidiManager.class);
    }

    void start() {
        if (manager == null) {
            return;
        }
        try {
            synchronized (lock) {
                if (!registered) {
                    manager.registerDeviceCallback(callback, handler);
                    registered = true;
                }
            }
            refresh();
        } catch (SecurityException ignored) {
            // USB MIDI remains available when Android requires Bluetooth permission.
        }
    }

    void refresh() {
        if (manager == null) {
            return;
        }
        try {
            for (MidiDeviceInfo info : manager.getDevices()) {
                openDevice(info);
            }
        } catch (SecurityException ignored) {
            // Permission can be granted later from the Audio settings.
        }
    }

    int[] portCounts() {
        synchronized (lock) {
            return new int[] {inputPorts.size(), outputPorts.size()};
        }
    }

    byte[] drainInputPackets() {
        List<byte[]> packets = new ArrayList<>(MAX_DRAINED_PACKETS);
        inputPackets.drainTo(packets, MAX_DRAINED_PACKETS);
        int size = 0;
        for (byte[] packet : packets) {
            size += 2 + packet.length;
        }
        byte[] output = new byte[size];
        int offset = 0;
        for (byte[] packet : packets) {
            output[offset++] = (byte) (packet.length >>> 8);
            output[offset++] = (byte) packet.length;
            System.arraycopy(packet, 0, output, offset, packet.length);
            offset += packet.length;
        }
        return output;
    }

    long droppedInputPackets() {
        return droppedPackets.get();
    }

    void send(byte[] bytes, long timestampNanos) {
        if (bytes == null || bytes.length == 0) {
            return;
        }
        MidiInputPort[] ports;
        synchronized (lock) {
            ports = outputPorts.values().toArray(new MidiInputPort[0]);
        }
        for (MidiInputPort port : ports) {
            try {
                port.send(bytes, 0, bytes.length, timestampNanos);
            } catch (IOException | IllegalStateException ignored) {
                // A device can disappear between the callback and this send.
            }
        }
    }

    private void openDevice(MidiDeviceInfo info) {
        if (manager == null) {
            return;
        }
        int id = info.getId();
        synchronized (lock) {
            if (devices.containsKey(id) || !opening.add(id)) {
                return;
            }
        }
        try {
            manager.openDevice(info, device -> {
                synchronized (lock) {
                    opening.remove(id);
                    if (device == null) {
                        return;
                    }
                    devices.put(id, device);
                    for (MidiDeviceInfo.PortInfo port : info.getPorts()) {
                        String key = id + ":" + port.getPortNumber();
                        if (port.getType() == MidiDeviceInfo.PortInfo.TYPE_OUTPUT) {
                            try {
                                MidiOutputPort source = device.openOutputPort(port.getPortNumber());
                                if (source != null) {
                                    source.connect(new MidiReceiver() {
                                        @Override
                                        public void onSend(byte[] message, int offset, int count, long timestamp)
                                                throws IOException {
                                            if (count <= 0) {
                                                return;
                                            }
                                            if (count > MAX_PACKET_BYTES) {
                                                droppedPackets.incrementAndGet();
                                                return;
                                            }
                                            byte[] packet = Arrays.copyOfRange(message, offset, offset + count);
                                            if (!inputPackets.offer(packet)) {
                                                droppedPackets.incrementAndGet();
                                            }
                                        }
                                    });
                                    inputPorts.put(key, source);
                                }
                            } catch (IllegalStateException ignored) {
                            }
                        } else if (port.getType() == MidiDeviceInfo.PortInfo.TYPE_INPUT) {
                            try {
                                MidiInputPort destination = device.openInputPort(port.getPortNumber());
                                if (destination != null) {
                                    outputPorts.put(key, destination);
                                }
                            } catch (IllegalStateException ignored) {
                            }
                        }
                    }
                }
            }, handler);
        } catch (SecurityException ignored) {
            synchronized (lock) {
                opening.remove(id);
            }
        }
    }

    private void closeDevice(int id) {
        synchronized (lock) {
            opening.remove(id);
            MidiDevice device = devices.remove(id);
            String prefix = id + ":";
            inputPorts.entrySet().removeIf(entry -> {
                if (!entry.getKey().startsWith(prefix)) {
                    return false;
                }
                try {
                    entry.getValue().close();
                } catch (IOException ignored) {
                }
                return true;
            });
            outputPorts.entrySet().removeIf(entry -> {
                if (!entry.getKey().startsWith(prefix)) {
                    return false;
                }
                try {
                    entry.getValue().close();
                } catch (IOException ignored) {
                }
                return true;
            });
            if (device != null) {
                try {
                    device.close();
                } catch (IOException ignored) {
                }
            }
        }
    }

    @Override
    public void close() {
        if (manager != null) {
            synchronized (lock) {
                if (registered) {
                    try {
                        manager.unregisterDeviceCallback(callback);
                    } catch (IllegalArgumentException ignored) {
                    }
                    registered = false;
                }
            }
        }
        Integer[] ids;
        synchronized (lock) {
            ids = devices.keySet().toArray(new Integer[0]);
        }
        for (int id : ids) {
            closeDevice(id);
        }
        inputPackets.clear();
    }
}
