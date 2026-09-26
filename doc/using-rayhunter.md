# Using Rayhunter

## Tactical LCD dashboard (Orbic / Moxee)

The [README screen gallery](../README.md#screens-from-the-connected-device)
shows the recording page on the physical Orbic LCD plus actual recording,
paused, waiting-for-data and screen-test warning framebuffer captures.

In **Config → Device UI Level**, select **Tactical dashboard (Orbic / Moxee)**
and save. This replaces the hotspot usage screen with a high-contrast field
dashboard. The equivalent configuration is `ui_level = 5`.

- **REC / RECORDING**: capture is running and diagnostic traffic arrived within
  the last 30 seconds. **WAIT DATA**, **PAUSED**, **ANALYSIS!**, and **DIAG ERROR**
  distinguish missing traffic, stopped capture, analysis failure, and capture
  task failure. Analysis failure remains flagged until a new recording because
  the session's counts may be incomplete. Zero alerts does not establish that
  the cellular network is safe.
- **SESSION ALERTS** and **H / M / L**: actual high-, medium-, and low-severity
  analyzer events for the current recording. Repeated events count individually;
  these are not counts of unique towers or confirmed cell-site simulators.
  Counts remain as **LAST RUN** while paused and reset on a new recording or
  daemon restart. Screen and LAN test alerts do not increment these counts.
  Values greater than 999 display as `999+`; recording reports retain the details.
- **RX**: elapsed time since diagnostic data was received, independent of the
  device's wall clock. **RUN**: recording duration, frozen while paused.
- **BAT**: device-reported battery level and charging state. Unknown readings
  are labeled rather than shown as zero. The Orbic reports coarse battery steps.
- **CONFIG HTTP :port**: combine a displayed IP with the port, for example
  `http://192.168.1.1:8080`, to open Rayhunter in a browser on that network.
  USB, Wi-Fi and LAN addresses refresh every five seconds. If more than two
  addresses exist, pairs rotate every five seconds. Only assigned private or
  link-local IPv4 addresses are shown; cellular bearer addresses are excluded.
  **NO LAN IP** or **IP READ FAILED** means no configuration address is available.

The existing flashing warning takes priority until acknowledged with a physical
button or the web interface; the dashboard then returns with its counts intact.
LCD power sequencing remains controlled by the vendor service. Press its button to wake
the screen. Tactical mode covers the stock menus; use the web configuration to
return to **Subtle mode** when you need the original hotspot interface.

Other device families retain their existing display behavior. Tactical mode
does not change hotspot, cellular, Wi-Fi, or X Suite LAN-alert settings.

Once installed, Rayhunter will run automatically whenever your device is running. You'll see a green line on top of the device's display to indicate that it's running and recording. [The line will turn yellow dots, orange dashes, or solid red](./faq.md#red) once a potential IMSI catcher has been found, depending on the severity of the alert, until the device is rebooted or a new recording is started through the web UI.

On Orbic and Moxee devices with **Screen Warning Alert** enabled, the first warning also wakes the display and flashes the configured warning text. Press a physical device button to acknowledge the flashing overlay. This only silences the overlay: recording continues and the warning status remains available on the display and in the web UI. A later higher-severity warning, or the first warning in a new recording, will alert again.

With **XTOC / XCOM LAN Alerts** enabled (the default), each warning is also sent directly to XTOC and XCOM computers on the same private LAN. Start either product with its packaged local launcher; its bundled receiver starts automatically and the app displays a latched English alert until acknowledged. No Internet service, cloud account, broker, pairing code, or destination address is required. To verify the complete path, open Rayhunter **Config** and select **Send test alert**.

For vehicle use, either connect the XTOC/XCOM computer to Rayhunter's hotspot or enable [WiFi client mode](./configuration.md#wifi-client-mode) and connect Rayhunter and the computer to the same router. Wi-Fi client mode needs the local SSID/password once, but alert delivery itself remains automatic. Guest/client-isolated networks may block broadcast; routed deployments can use an optional direct receiver URL.

![Rayhunter_0 5 0](./Rayhunter_0.5.0.png)

It also serves a web UI that provides some basic controls, such as being able to start/stop recordings, download captures, delete captures, and view heuristic analyses of captures.

## The web UI

You can access this UI in one of two ways:

* **Connect over WiFi:** Connect your phone/laptop to your device's WiFi
  network and visit <http://192.168.1.1:8080> (orbic)
  or <http://192.168.0.1:8080> (tplink).

  Click past your browser warning you about the connection not being secure; Rayhunter doesn't have HTTPS yet.

  On the **Orbic**, you can find the WiFi network password by going to the Orbic's menu > 2.4 GHz WIFI Info > Enter > find the 8-character password next to the lock 🔒 icon.
  On the **TP-Link**, you can find the WiFi network password by going to the TP-Link's menu > Advanced > Wireless > Basic Settings.

  If [WiFi client mode](./configuration.md#wifi-client-mode) is enabled, you can also reach the web UI from any device on that network at `http://<device-ip>:8080`.

* **Connect over USB (Orbic):** Connect your device to your laptop via USB. Run `adb forward tcp:8080 tcp:8080`, then visit <http://localhost:8080>.
    * For this you will need to install the Android Debug Bridge (ADB) on your computer, you can copy the version that was downloaded inside the `releases/platform-tools/` folder to somewhere else in your path or you can install it manually.
    * You can find instructions for doing so on your platform [here](https://www.xda-developers.com/install-adb-windows-macos-linux/#how-to-set-up-adb-on-your-computer), (don't worry about instructions for installing it on a phone/device yet).
    * On MacOS, the easiest way to install ADB is with Homebrew: First [install Homebrew](https://brew.sh/), then run `brew install android-platform-tools`.

* **Connect over USB (TP-Link):** Plug in the TP-Link and use USB tethering to establish a network connection. ADB support can be enabled on the device, but the installer won't do it for you.

> **_NOTE:_** When downloading recordings, "Insecure download blocked" warnings can safely be ignored - this is due to Rayhunter not using HTTPS.

## Key shortcuts

As of Rayhunter version 0.3.3, you can start a new recording by double-tapping the power button. Any current recording will be stopped and a new recording will be started, resetting the red line as well. This feature is disabled by default since Rayhunter version 0.4.0 and needs to be enabled through [configuration](./configuration.md).

### Orbic alert-clear recovery (September 23, 2026)

The owner observed a white, stuck physical panel after clearing an alert in the
first tactical build. A correct framebuffer readback did not mean the physical
screen worked. A power cycle restored the dashboard. The revised build requests
wake using the existing Orbic display service and never resets the LCD or
restores stale backlight/blanking values. See the [validation record](tactical-lcd-validation.md)
for the installed binary hash and actual hardware results. If the vendor utility
is unavailable, automatic wake is unverified; press Power/OK to wake the display.
