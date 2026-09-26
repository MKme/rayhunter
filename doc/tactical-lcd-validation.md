# Tactical LCD validation — September 23–26, 2026

## Hardware failure and correction

The first tactical build produced correct framebuffer captures, but the owner
reported that clearing its screen-only test made the physical panel fade white
and stick. Direct LCD reinitialization did not recover it. The owner power
cycled the unit and confirmed the dashboard returned. **Framebuffer/sysfs/API
success alone is not a physical-display pass.** No factory reset was performed.

The revised Orbic alert path uses the already-installed `/usr/bin/qt_test`
command: option 24 (screen status), value 1 (awake), then -1 (exit). The request
passes through the vendor display service instead of directly writing LCD
initialization, blanking, display-on or backlight controls. The owner confirmed
that this service-based wake showed the dashboard normally before installation.

While an alert flashes, the daemon requests wake at most every five seconds.
Each subprocess has a 750-millisecond timeout and is killed on cancellation. A missing
utility, failure or unexpected reply is logged; there is no raw-GPIO fallback.
Acknowledgment requests wake and repaints the current tactical dashboard,
retaining recording and alert counts. Other UI modes restore their saved pixels.
Physical wake on Moxee is unverified and must be checked on that hardware.

## Installed artifact

Path: `/data/rayhunter/rayhunter-daemon` (Orbic, version 0.12.0, `ui_level = 5`).
Host build: `target/armv7-unknown-linux-musleabihf/firmware-devel/rayhunter-daemon`.
Size: **16,841,532 bytes**. SHA-256:
`2354131ebf36c8708ad9eef72ccc3491c3f6aaa07e6a47a1da18a7a3f7e5112d`.
The installed hash matches the local binary.

The earlier, physically failing tactical build had SHA-256
`a3412f5588e6094664ea992cf8c7b958b2192438cd56e7e43dd6139ca614a8d7`.
It is superseded by the service-based wake build above.

## Preservation and local validation

- All 55 daemon library tests passed **on the actual ARM Orbic**, including the
  vendor wake reply and throttling checks, alert acknowledgment repaint,
  screen-alert controller, button input, API, recording helpers and LAN encoders.
  Temporary-file tests used a dedicated device directory; they did not alter
  user recordings. This is software validation, not proof of LCD appearance.
- Six pure tactical model/renderer tests passed on Windows: repeated detections,
  pause/reset, stale traffic, analysis/capture failure, saturation, address
  filtering and multipage rendering.
- Web tests: five passed. Svelte check: zero errors and zero warnings. Existing
  Vite import.meta/IIFE warnings remain; standard-library gzip generated the
  embedded web bundle where the Windows shell lacked gzip.
- Offline, locked ARM firmware build passed with the existing dependencies.
  No external assets, packages or services were added.
- Before the original update, all 25 existing QMDL files passed SHA-256
  preservation checks. The initial setting change was only `ui_level: 1 → 5`.
- Before installing the wake fix, the recording was closed through its API and
  the old daemon stopped. All **28** then-existing capture files retained their
  hashes after installation. The entire configuration retained its hash.
  The new daemon resumed recording through the unchanged init script.
- The final timeout-bounded build preserved all **30** then-existing capture
  files and the full configuration hash. Its 55 native tests also exercise a
  missing utility, rejected reply, nonzero exit and hung subprocess.
- The owner confirmed no factory reset. Hotspot credentials, Wi-Fi, LAN-alert
  settings and existing captures were preserved. No LAN test notification or
  real detection was manufactured for the LCD tests.
- A reproducible pause/resume test verified PAUSED/LAST RUN, frozen run time,
  reset session counters, WAIT DATA/RX NONE, unchanged configuration and capture
  preservation. The screenshots in the README come from those actual frames.

## Display evidence and limits

The README includes actual 128 × 128 framebuffer captures for recording, paused,
waiting for initial data and a screen-only warning. Images are enlarged with
nearest-neighbor pixels. The separate composite preview has illustrative counts
and addresses, explicitly labeled as examples. No screenshot claims an actual
cell-site-simulator detection.

On September 26, the owner supplied a camera photograph of the connected Orbic
showing the tactical **RECORDING** page on the physical LCD. The page visibly
shows zero session alerts, current RX and run ages, charging state, and the LAN
and USB configuration addresses. This confirms physical rendering of the new
recording page; it does not establish cellular-network safety or manufacture a
detection. The photograph is stored as
`doc/images/tactical-lcd-device-recording.png`.

Counters are non-informational analyzer events in the current/last recording;
repeated events count separately. Tests cannot change the detection counters.
A new recording clears counts. Analysis failure is latched until a new recording;
capture-task failure reports DIAG ERROR. RX/RUN use a monotonic clock, independent
of the device's incorrect wall clock. Refresh is four times per second, with
battery and address enumeration every five seconds.

The revised firmware was tested from vendor sleep (backlight 0), flashed for
35 seconds (all seven five-second samples showed backlight 1), and acknowledged
successfully through the web API. Recording remained active and a new framebuffer
was captured after clear. The September 26 camera photograph confirms that the
tactical recording page renders on the physical LCD.

The final build's additional start/clear check completed in 125 ms / 62 ms.
The same recording continued, increasing from 2,502 to 2,774 capture bytes.
The README after-clear image is from this final build. A physical-button clear
on the revised build remains unverified; software button-input tests are not
a substitute for that hardware check.

## Reproduction

Keep device credentials in private local records, never in committed docs. The
default Orbic Rayhunter interface is `http://192.168.1.1:8080`. Open an existing
authorized temporary maintenance connection only when needed, supplying the
admin password interactively or from a private environment variable:

```powershell
.\target\debug\installer.exe util orbic-start-telnet --admin-password '<private-admin-password>'
cargo test --offline --locked -p rayhunter-daemon --lib --target armv7-unknown-linux-musleabihf --profile firmware-devel --no-run
cargo build --offline --locked -p rayhunter-daemon --bin rayhunter-daemon --target armv7-unknown-linux-musleabihf --profile firmware-devel
python tools/test-tactical-device.py --output device-private/new-validation-run
```

The last command briefly stops and starts recording and requires an output
folder that does not already exist. It never deletes captures or changes
configuration. Upload the compiled native test executable to a dedicated device
temporary directory using `tools/orbic-maintenance.py put`; use that directory
as TMPDIR when running it with `--test-threads=1`. Preserve the returned test log.
Use **Test Screen Alert**, let it flash beyond the stock screen timeout, then
clear with **Stop Alert** and separately test a physical Power/OK or Menu press.
Check the actual LCD after each clear, not just framebuffer contents.

## Rollback and evidence

The device retains `/data/rayhunter/backup-tactical-20260923/` with the previous
pre-tactical daemon, configuration and init script. It is an intentional rollback
copy. `/data/rayhunter` still links to `/data/rayhunter-data`; no migration ran.

Private logs, capture hashes, configuration readbacks and raw framebuffer
captures remain in the Git-ignored `device-private/` directory. New installation
evidence is in `vendor-fix-final-install.json`, `native-final-vendor-wake-tests.txt`,
`vendor-fix-alert-test.json` and `vendor-fix-final-alert-test.json`.
No CI workflows were created or triggered during validation. Device credentials
and private evidence remain excluded from the repository.

Temporary device test binaries and their directory, host vendor-binary
inspection copies, and the helper's Python cache were removed. Maintenance
port 24 was closed and checked separately while the Rayhunter API and recording
remained active. The rollback directory is deliberately retained on the device.
