# Audio item loop-copy destination tracks

The native probe used Linux REAPER 7.82 / Default 7, a fresh isolated profile, an unsaved stopped project, and two selected 0–10 second source items on named tracks. It copied the 3–7 second selection, deleted source track B, then pasted at 12 seconds. REAPER kept the first pasted item on track A and created a blank track for the second item at the same relative track offset. After deleting all tracks, pasting the same clipboard at 30 seconds created two blank tracks with the two selected 4-second items.

AAADAW's Paste command now remains enabled with an empty track list and creates the missing blank destination tracks. Track creation and item insertion share one project transaction, so Undo removes both the created tracks and pasted items and Redo restores them. Existing tracks are reused at their indexed offsets; gaps are filled with blank tracks, matching the native two-track case.

The [native ReaScript output](fixtures/item-copy-loop-track-growth-native.tsv) records both scenarios. This was an action probe; no additional window screenshot was taken. The audio clipboard remains project-local, copies across an already-looped source boundary are still rejected, and non-unity Take-rate source mapping remains unsupported.
