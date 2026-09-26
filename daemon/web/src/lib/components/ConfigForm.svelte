<script lang="ts">
    import {
        get_config,
        set_config,
        test_notification,
        test_screen_alert,
        acknowledge_screen_alert,
        test_xsuite_alert,
        get_xsuite_alert_status,
        get_wifi_status,
        scan_wifi_networks,
        GpsMode,
        enabled_notifications,
        type Config,
        type WifiStatus,
        type WifiNetwork,
        type XsuiteAlertStatus,
    } from '../utils.svelte';
    import Modal from './Modal.svelte';
    import ExpandableInput from './ExpandableInput.svelte';

    let { shown = $bindable() }: { shown: boolean } = $props();
    let config = $state<Config | null>(null);

    let loading = $state(false);
    let saving = $state(false);
    let testingNotification = $state(false);
    let testingScreenAlert = $state(false);
    let stoppingScreenAlert = $state(false);
    let message = $state('');
    let messageType = $state<'success' | 'error' | null>(null);
    let testMessage = $state('');
    let testMessageType = $state<'success' | 'error' | null>(null);
    let screenAlertMessage = $state('');
    let screenAlertMessageType = $state<'success' | 'error' | null>(null);
    let wifiStatus = $state<WifiStatus | null>(null);
    let wifiStatusTimer = $state<ReturnType<typeof setInterval> | null>(null);
    let scanning = $state(false);
    let scanResults = $state<WifiNetwork[]>([]);
    let dnsServersInput = $state('');
    let xsuiteDestinationsInput = $state('');
    let testingXsuiteAlert = $state(false);
    let xsuiteTestMessage = $state('');
    let xsuiteTestMessageType = $state<'success' | 'error' | null>(null);
    let xsuiteStatus = $state<XsuiteAlertStatus | null>(null);

    async function load_config() {
        try {
            loading = true;
            config = await get_config();
            dnsServersInput = config.dns_servers ? config.dns_servers.join(', ') : '';
            xsuiteDestinationsInput = config.xsuite_alerts.destinations.join('\n');
            try {
                xsuiteStatus = await get_xsuite_alert_status();
            } catch {
                xsuiteStatus = null;
            }
            message = '';
            messageType = null;
            screenAlertMessage = '';
            screenAlertMessageType = null;
            poll_wifi_status();
        } catch (error) {
            message = `Failed to load config: ${error}`;
            messageType = 'error';
        } finally {
            loading = false;
        }
    }

    async function save_config() {
        if (!config) return;

        const trimmed = dnsServersInput.trim();
        config.dns_servers =
            trimmed.length > 0
                ? trimmed
                      .split(',')
                      .map((s) => s.trim())
                      .filter((s) => s.length > 0)
                : null;
        config.xsuite_alerts.destinations = xsuiteDestinationsInput
            .split(/[\n,]/)
            .map((value) => value.trim())
            .filter((value) => value.length > 0);

        try {
            saving = true;
            await set_config(config);
            message =
                'Config saved successfully! Rayhunter is restarting now. Reload the page in a few seconds.';
            messageType = 'success';
        } catch (error) {
            message = `Failed to save config: ${error}`;
            messageType = 'error';
        } finally {
            saving = false;
        }
    }

    async function poll_wifi_status() {
        if (wifiStatusTimer) clearInterval(wifiStatusTimer);
        try {
            wifiStatus = await get_wifi_status();
        } catch {
            wifiStatus = null;
        }
        wifiStatusTimer = setInterval(async () => {
            try {
                wifiStatus = await get_wifi_status();
            } catch {
                wifiStatus = null;
            }
        }, 5000);
    }

    let scanError = $state('');

    async function do_scan() {
        scanning = true;
        scanError = '';
        try {
            scanResults = await scan_wifi_networks();
        } catch (error) {
            scanResults = [];
            scanError = `Scan failed: ${error}`;
        } finally {
            scanning = false;
        }
    }

    function select_network(network: WifiNetwork) {
        if (config) {
            config.wifi_ssid = network.ssid;
            config.wifi_password = '';
            config.wifi_security =
                network.security === 'WPA3' || network.security === 'WPA3 (transition)'
                    ? 'sae'
                    : 'wpa_psk';
            scanResults = [];
        }
    }

    async function send_test_notification() {
        try {
            testingNotification = true;
            testMessage = '';
            testMessageType = null;
            await test_notification();
            testMessage = 'Test notification sent successfully!';
            testMessageType = 'success';
        } catch (error) {
            testMessage = `${error}`;
            testMessageType = 'error';
        } finally {
            testingNotification = false;
        }
    }

    async function send_test_xsuite_alert() {
        try {
            testingXsuiteAlert = true;
            xsuiteTestMessage = '';
            xsuiteTestMessageType = null;
            const report = await test_xsuite_alert();
            xsuiteTestMessage = `Test alert sent (${report.packet_count} packet${report.packet_count === 1 ? '' : 's'}, ${report.broadcast_datagrams} LAN broadcast${report.broadcast_datagrams === 1 ? '' : 's'}). Correlation ${report.correlation_id}.`;
            xsuiteTestMessageType = 'success';
            xsuiteStatus = await get_xsuite_alert_status();
        } catch (error) {
            xsuiteTestMessage = `Test failed: ${error}`;
            xsuiteTestMessageType = 'error';
        } finally {
            testingXsuiteAlert = false;
        }
    }

    async function start_test_screen_alert() {
        try {
            testingScreenAlert = true;
            screenAlertMessage = '';
            screenAlertMessageType = null;
            await test_screen_alert();
            screenAlertMessage =
                'Screen alert started. It will continue until you press a device button or select Stop Alert.';
            screenAlertMessageType = 'success';
        } catch (error) {
            screenAlertMessage = `Failed to start screen alert: ${error}`;
            screenAlertMessageType = 'error';
        } finally {
            testingScreenAlert = false;
        }
    }

    async function stop_test_screen_alert() {
        try {
            stoppingScreenAlert = true;
            screenAlertMessage = '';
            screenAlertMessageType = null;
            await acknowledge_screen_alert();
            screenAlertMessage = 'Screen alert stopped.';
            screenAlertMessageType = 'success';
        } catch (error) {
            screenAlertMessage = `Failed to stop screen alert: ${error}`;
            screenAlertMessageType = 'error';
        } finally {
            stoppingScreenAlert = false;
        }
    }

    $effect(() => {
        if (shown && !config) {
            load_config();
        }
        if (!shown && wifiStatusTimer) {
            clearInterval(wifiStatusTimer);
            wifiStatusTimer = null;
        }
        return () => {
            if (wifiStatusTimer) {
                clearInterval(wifiStatusTimer);
                wifiStatusTimer = null;
            }
        };
    });
</script>

<Modal bind:shown title="Configuration">
    <div class="p-2">
        {#if loading}
            <div class="text-center py-4">Loading config...</div>
        {:else if config}
            <form
                class="space-y-4"
                onsubmit={(e) => {
                    e.preventDefault();
                    save_config();
                }}
            >
                <div>
                    <label for="ui_level" class="block text-sm font-medium text-gray-700 mb-1">
                        Device UI Level
                    </label>
                    <select
                        id="ui_level"
                        bind:value={config.ui_level}
                        class="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-hidden focus:ring-2 focus:ring-rayhunter-blue"
                    >
                        <option value={0}>Invisible mode</option>
                        <option value={1}>Subtle mode (colored line)</option>
                        <option value={2}>Demo mode (orca gif)</option>
                        <option value={3}>EFF logo</option>
                        <option value={4}>High visibility (full screen color)</option>
                        {#if config.device === 'orbic' || config.device === 'moxee'}
                            <option value={5}>Tactical dashboard (Orbic / Moxee)</option>
                        {/if}
                    </select>
                    <p class="text-xs text-gray-500 mt-1">
                        Note: Rayhunter draws over the device's native UI, so some flickering is
                        expected
                    </p>
                </div>

                {#if config.device === 'orbic' || config.device === 'moxee'}
                    <div class="border-t border-gray-200 pt-4 mt-6 space-y-3">
                        <h3 class="text-lg font-semibold text-gray-800">Screen Warning Alert</h3>

                        <div class="flex items-center">
                            <input
                                id="screen_alert_enabled"
                                type="checkbox"
                                bind:checked={config.screen_alert.enabled}
                                class="h-4 w-4 text-rayhunter-blue focus:ring-rayhunter-blue border-gray-300 rounded-sm"
                            />
                            <label
                                for="screen_alert_enabled"
                                class="ml-2 block text-sm text-gray-700"
                            >
                                Wake and flash the screen on warnings
                            </label>
                        </div>

                        <div>
                            <label
                                for="screen_alert_message"
                                class="block text-sm font-medium text-gray-700 mb-1"
                            >
                                Alert message
                            </label>
                            <textarea
                                id="screen_alert_message"
                                bind:value={config.screen_alert.message}
                                maxlength="80"
                                rows="3"
                                placeholder="SUSPICIOUS CELLULAR DEVICE DETECTED"
                                class="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-hidden focus:ring-2 focus:ring-rayhunter-blue"
                            ></textarea>
                            <div class="flex justify-between gap-3 mt-1 text-xs text-gray-500">
                                <p>
                                    Displayed in uppercase and wrapped to fit the 128 &times; 128
                                    device screen.
                                </p>
                                <span class="shrink-0">{config.screen_alert.message.length}/80</span
                                >
                            </div>
                        </div>

                        <p class="text-xs text-amber-700">
                            Save settings with Apply and restart before testing. The test uses the
                            last saved setting and message.
                        </p>

                        <div class="flex flex-wrap gap-2">
                            <button
                                type="button"
                                onclick={start_test_screen_alert}
                                disabled={testingScreenAlert || stoppingScreenAlert}
                                class="bg-rayhunter-blue hover:bg-rayhunter-dark-blue disabled:opacity-50 disabled:cursor-not-allowed text-white font-bold py-2 px-4 rounded-md flex flex-row gap-1 items-center"
                            >
                                {#if testingScreenAlert}
                                    <div
                                        class="w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin"
                                    ></div>
                                    Starting...
                                {:else}
                                    Test Screen Alert
                                {/if}
                            </button>
                            <button
                                type="button"
                                onclick={stop_test_screen_alert}
                                disabled={testingScreenAlert || stoppingScreenAlert}
                                class="bg-gray-100 hover:bg-gray-200 disabled:opacity-50 disabled:cursor-not-allowed text-gray-800 font-bold py-2 px-4 border border-gray-300 rounded-md flex flex-row gap-1 items-center"
                            >
                                {#if stoppingScreenAlert}
                                    <div
                                        class="w-4 h-4 border-2 border-gray-600 border-t-transparent rounded-full animate-spin"
                                    ></div>
                                    Stopping...
                                {:else}
                                    Stop Alert
                                {/if}
                            </button>
                        </div>

                        {#if screenAlertMessage}
                            <div
                                aria-live="polite"
                                class="p-2 rounded-sm text-sm {screenAlertMessageType === 'error'
                                    ? 'bg-red-100 text-red-700'
                                    : 'bg-green-100 text-green-700'}"
                            >
                                {screenAlertMessage}
                            </div>
                        {/if}
                    </div>
                {/if}

                <div>
                    <label
                        for="key_input_mode"
                        class="block text-sm font-medium text-gray-700 mb-1"
                    >
                        Device Input Mode
                    </label>
                    <select
                        id="key_input_mode"
                        bind:value={config.key_input_mode}
                        class="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-hidden focus:ring-2 focus:ring-rayhunter-blue"
                    >
                        <option value={0}>Disable button control</option>
                        <option value={1}>Double-tap power button to start new recording</option>
                    </select>
                </div>

                <div class="space-y-3">
                    <div class="flex items-center">
                        <input
                            id="colorblind_mode"
                            type="checkbox"
                            bind:checked={config.colorblind_mode}
                            class="h-4 w-4 text-rayhunter-blue focus:ring-rayhunter-blue border-gray-300 rounded-sm"
                        />
                        <label for="colorblind_mode" class="ml-2 block text-sm text-gray-700">
                            Colorblind Mode
                        </label>
                    </div>
                </div>

                <div class="border-t border-gray-200 pt-4 mt-6 space-y-4">
                    <div>
                        <h3 class="text-lg font-semibold text-gray-800">XTOC / XCOM LAN Alerts</h3>
                        <p class="text-xs text-gray-500 mt-1">
                            Plug-and-play is enabled by default. Rayhunter broadcasts native X1
                            EVENT packets on the local LAN; a current XTOC or XCOM local launcher
                            receives and displays them automatically. Internet access is not needed.
                        </p>
                    </div>

                    <div class="flex items-center">
                        <input
                            id="xsuite_alerts_enabled"
                            type="checkbox"
                            bind:checked={config.xsuite_alerts.enabled}
                            class="h-4 w-4 text-rayhunter-blue focus:ring-rayhunter-blue border-gray-300 rounded-sm"
                        />
                        <label for="xsuite_alerts_enabled" class="ml-2 block text-sm text-gray-700">
                            Send detection alerts to XTOC and XCOM
                        </label>
                    </div>

                    <div class="flex items-center">
                        <input
                            id="xsuite_broadcast_enabled"
                            type="checkbox"
                            bind:checked={config.xsuite_alerts.broadcast_enabled}
                            class="h-4 w-4 text-rayhunter-blue focus:ring-rayhunter-blue border-gray-300 rounded-sm"
                        />
                        <label for="xsuite_broadcast_enabled" class="ml-2 block text-sm text-gray-700">
                            Automatic LAN broadcast (recommended)
                        </label>
                    </div>

                    <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                        <div>
                            <label for="xsuite_device_label" class="block text-sm font-medium text-gray-700 mb-1">
                                Device label
                            </label>
                            <input
                                id="xsuite_device_label"
                                type="text"
                                maxlength="32"
                                bind:value={config.xsuite_alerts.device_label}
                                class="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-hidden focus:ring-2 focus:ring-rayhunter-blue"
                            />
                        </div>
                        <div>
                            <label for="xsuite_minimum_severity" class="block text-sm font-medium text-gray-700 mb-1">
                                Minimum severity
                            </label>
                            <select
                                id="xsuite_minimum_severity"
                                bind:value={config.xsuite_alerts.minimum_severity}
                                class="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-hidden focus:ring-2 focus:ring-rayhunter-blue"
                            >
                                <option value="Low">Low and above</option>
                                <option value="Medium">Medium and above</option>
                                <option value="High">High only</option>
                            </select>
                        </div>
                        <div>
                            <label for="xsuite_source_unit" class="block text-sm font-medium text-gray-700 mb-1">
                                X Suite source unit ID
                            </label>
                            <input
                                id="xsuite_source_unit"
                                type="number"
                                min="1"
                                max="65535"
                                bind:value={config.xsuite_alerts.source_unit_id}
                                class="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-hidden focus:ring-2 focus:ring-rayhunter-blue"
                            />
                        </div>
                        <div>
                            <label for="xsuite_broadcast_port" class="block text-sm font-medium text-gray-700 mb-1">
                                LAN alert port
                            </label>
                            <input
                                id="xsuite_broadcast_port"
                                type="number"
                                min="1"
                                max="65535"
                                bind:value={config.xsuite_alerts.broadcast_port}
                                class="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-hidden focus:ring-2 focus:ring-rayhunter-blue"
                            />
                        </div>
                    </div>

                    <div class="flex items-center">
                        <input
                            id="xsuite_include_sentinel"
                            type="checkbox"
                            bind:checked={config.xsuite_alerts.include_sentinel_packet}
                            class="h-4 w-4 text-rayhunter-blue focus:ring-rayhunter-blue border-gray-300 rounded-sm"
                        />
                        <label for="xsuite_include_sentinel" class="ml-2 block text-sm text-gray-700">
                            Also send a Sentinel packet when valid GPS coordinates are available
                        </label>
                    </div>

                    <div class="flex items-center">
                        <input
                            id="xsuite_include_message"
                            type="checkbox"
                            bind:checked={config.xsuite_alerts.include_full_message}
                            class="h-4 w-4 text-rayhunter-blue focus:ring-rayhunter-blue border-gray-300 rounded-sm"
                        />
                        <label for="xsuite_include_message" class="ml-2 block text-sm text-gray-700">
                            Include full detection details on the LAN
                        </label>
                    </div>

                    <div>
                        <label for="xsuite_destinations" class="block text-sm font-medium text-gray-700 mb-1">
                            Optional direct receiver URLs
                        </label>
                        <textarea
                            id="xsuite_destinations"
                            rows="2"
                            bind:value={xsuiteDestinationsInput}
                            placeholder="http://192.168.50.10:8095"
                            class="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-hidden focus:ring-2 focus:ring-rayhunter-blue"
                        ></textarea>
                        <p class="text-xs text-gray-500 mt-1">
                            Usually leave this blank. Add one URL per line only for routed networks
                            that block LAN broadcasts.
                        </p>
                    </div>

                    <div class="flex flex-wrap items-center gap-3">
                        <button
                            type="button"
                            onclick={send_test_xsuite_alert}
                            disabled={testingXsuiteAlert}
                            class="bg-red-600 hover:bg-red-700 disabled:opacity-50 disabled:cursor-not-allowed text-white font-bold py-2 px-4 rounded-md"
                        >
                            {testingXsuiteAlert ? 'Sending test...' : 'Send XTOC / XCOM Test Alert'}
                        </button>
                        {#if xsuiteStatus}
                            <span class="text-xs text-gray-500">
                                Sent: {xsuiteStatus.sent_alerts} &middot; Deduplicated: {xsuiteStatus.deduped_alerts}
                                {#if xsuiteStatus.last_success_at}
                                    &middot; Last success: {new Date(xsuiteStatus.last_success_at).toLocaleString()}
                                {/if}
                                {#if xsuiteStatus.last_error}
                                    &middot; Last error: {xsuiteStatus.last_error}
                                {/if}
                            </span>
                        {/if}
                    </div>
                    <p class="text-xs text-amber-700">
                        The test uses the currently saved settings. Save and let Rayhunter restart
                        before testing changes made above.
                    </p>
                    {#if xsuiteTestMessage}
                        <div
                            aria-live="polite"
                            class="p-2 rounded-sm text-sm {xsuiteTestMessageType === 'error'
                                ? 'bg-red-100 text-red-700'
                                : 'bg-green-100 text-green-700'}"
                        >
                            {xsuiteTestMessage}
                        </div>
                    {/if}
                </div>

                <div class="border-t border-gray-200 pt-4 mt-6 space-y-3">
                    <h3 class="text-lg font-semibold text-gray-800 mb-4">Other Notifications</h3>

                    <div class="flex items-center">
                        <input
                            id="auto_check_updates"
                            type="checkbox"
                            bind:checked={config.auto_check_updates}
                            class="h-4 w-4 text-rayhunter-blue focus:ring-rayhunter-blue border-gray-300 rounded-sm"
                        />
                        <label for="auto_check_updates" class="ml-2 block text-sm text-gray-700">
                            Automatically check for software updates
                        </label>
                    </div>
                    <p class="text-xs text-gray-500">
                        When enabled, Rayhunter periodically checks GitHub for new releases and
                        shows an update notice in the web UI.
                    </p>

                    <ExpandableInput
                        bind:value={config.ntfy_url}
                        checkboxId="ntfy_enabled"
                        inputId="ntfy_url"
                        label="Enable ntfy notifications"
                        inputLabel="ntfy URL"
                        inputPlaceholder="https://ntfy.sh/my-rayhunter"
                        inputHelp="Test button below uses the saved configuration URL, not the input above"
                    >
                        <div>
                            <button
                                type="button"
                                onclick={send_test_notification}
                                disabled={testingNotification}
                                class="bg-rayhunter-blue hover:bg-rayhunter-dark-blue disabled:opacity-50 disabled:cursor-not-allowed text-white font-bold py-2 px-4 rounded-md flex flex-row gap-1 items-center"
                            >
                                {#if testingNotification}
                                    <div
                                        class="w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin"
                                    ></div>
                                    Sending...
                                {:else}
                                    <svg
                                        class="w-4 h-4"
                                        fill="none"
                                        stroke="currentColor"
                                        viewBox="0 0 24 24"
                                    >
                                        <path
                                            stroke-linecap="round"
                                            stroke-linejoin="round"
                                            stroke-width="2"
                                            d="M12 19l9 2-9-18-9 18 9-2zm0 0v-8"
                                        ></path>
                                    </svg>
                                    Send Test Notification
                                {/if}
                            </button>
                            {#if testMessage}
                                <div
                                    class="mt-2 p-2 rounded-sm text-sm {testMessageType === 'error'
                                        ? 'bg-red-100 text-red-700'
                                        : 'bg-green-100 text-green-700'}"
                                >
                                    {testMessage}
                                </div>
                            {/if}
                        </div>

                        <div class="space-y-2">
                            <div class="block text-sm font-medium text-gray-700 mb-1">
                                Enabled Notification Types
                            </div>
                            <div class="flex items-center">
                                <input
                                    type="checkbox"
                                    id="enable_warning_notifications"
                                    value="Warning"
                                    bind:group={config.enabled_notifications}
                                />
                                <label
                                    for="enable_warning_notifications"
                                    class="ml-2 block text-sm text-gray-700"
                                >
                                    Warnings
                                </label>
                            </div>
                            <div class="flex items-center">
                                <input
                                    type="checkbox"
                                    id="enable_lowbattery_notifications"
                                    value="LowBattery"
                                    bind:group={config.enabled_notifications}
                                />
                                <label
                                    for="enable_lowbattery_notifications"
                                    class="ml-2 block text-sm text-gray-700"
                                >
                                    Low Battery
                                </label>
                            </div>
                            <div class="flex items-center">
                                <input
                                    type="checkbox"
                                    id="enable_update_notifications"
                                    value={enabled_notifications.Update}
                                    bind:group={config.enabled_notifications}
                                />
                                <label
                                    for="enable_update_notifications"
                                    class="ml-2 block text-sm text-gray-700"
                                >
                                    Software Updates
                                </label>
                            </div>
                        </div>
                    </ExpandableInput>
                </div>

                <div class="border-t border-gray-200 pt-4 mt-6 space-y-3">
                    <h3 class="text-lg font-semibold text-gray-800 mb-4">Storage Management</h3>

                    <div>
                        <label
                            for="min_space_to_start_recording_mb"
                            class="block text-sm font-medium text-gray-700 mb-1"
                        >
                            Minimum Space to Start Recording (MB)
                        </label>
                        <input
                            id="min_space_to_start_recording_mb"
                            type="number"
                            min="1"
                            bind:value={config.min_space_to_start_recording_mb}
                            class="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-hidden focus:ring-2 focus:ring-rayhunter-blue"
                        />
                        <p class="text-xs text-gray-500 mt-1">
                            Recording will not start if less than this amount of disk space is free
                        </p>
                    </div>

                    <div>
                        <label
                            for="min_space_to_continue_recording_mb"
                            class="block text-sm font-medium text-gray-700 mb-1"
                        >
                            Minimum Space to Continue Recording (MB)
                        </label>
                        <input
                            id="min_space_to_continue_recording_mb"
                            type="number"
                            min="1"
                            bind:value={config.min_space_to_continue_recording_mb}
                            class="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-hidden focus:ring-2 focus:ring-rayhunter-blue"
                        />
                        <p class="text-xs text-gray-500 mt-1">
                            Recording will stop automatically if disk space drops below this level
                        </p>
                    </div>
                </div>

                <div class="border-t border-gray-200 pt-4 mt-6 space-y-3">
                    <h3 class="text-lg font-semibold text-gray-800 mb-4">WebDAV Upload</h3>
                    <p class="text-xs text-gray-500">
                        Once a recording has been closed for at least the configured age, both the
                        .qmdl and .ndjson files are uploaded in the background to the WebDAV server.
                    </p>

                    <ExpandableInput
                        bind:value={config.webdav.url}
                        checkboxId="webdav_enabled"
                        inputId="webdav_url"
                        label="Enable WebDAV upload"
                        inputLabel="Server URL"
                        inputPlaceholder="https://dav.example.com/rayhunter/"
                        inputHelp="Files are uploaded via HTTP PUT under this base URL. No folders are created, and folders in this base URL are assumed to exist already."
                    >
                        <div>
                            <label
                                for="webdav_username"
                                class="block text-sm font-medium text-gray-700 mb-1"
                            >
                                Username
                            </label>
                            <input
                                id="webdav_username"
                                type="text"
                                bind:value={config.webdav.username}
                                class="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-hidden focus:ring-2 focus:ring-rayhunter-blue"
                            />
                            <p class="text-xs text-gray-500 mt-1">
                                Optional. Leave blank for unauthenticated uploads.
                            </p>
                        </div>

                        <div>
                            <label
                                for="webdav_password"
                                class="block text-sm font-medium text-gray-700 mb-1"
                            >
                                Password
                            </label>
                            <input
                                id="webdav_password"
                                type="password"
                                bind:value={config.webdav.password}
                                class="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-hidden focus:ring-2 focus:ring-rayhunter-blue"
                            />
                            <p class="text-xs text-gray-500 mt-1">
                                A password without a username will be rejected and the request will
                                be sent unauthenticated.
                            </p>
                        </div>

                        <div>
                            <label
                                for="webdav_upload_timeout_secs"
                                class="block text-sm font-medium text-gray-700 mb-1"
                            >
                                Upload Timeout (seconds)
                            </label>
                            <input
                                id="webdav_upload_timeout_secs"
                                type="number"
                                min="1"
                                bind:value={config.webdav.upload_timeout_secs}
                                class="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-hidden focus:ring-2 focus:ring-rayhunter-blue"
                            />
                        </div>

                        <div>
                            <label
                                for="webdav_poll_interval_secs"
                                class="block text-sm font-medium text-gray-700 mb-1"
                            >
                                Poll Interval (seconds)
                            </label>
                            <input
                                id="webdav_poll_interval_secs"
                                type="number"
                                min="1"
                                bind:value={config.webdav.poll_interval_secs}
                                class="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-hidden focus:ring-2 focus:ring-rayhunter-blue"
                            />
                            <p class="text-xs text-gray-500 mt-1">
                                How often the worker checks for new entries to upload.
                            </p>
                        </div>

                        <div>
                            <label
                                for="webdav_min_age_secs"
                                class="block text-sm font-medium text-gray-700 mb-1"
                            >
                                Minimum Age Before Upload (seconds)
                            </label>
                            <input
                                id="webdav_min_age_secs"
                                type="number"
                                min="0"
                                bind:value={config.webdav.min_age_secs}
                                class="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-hidden focus:ring-2 focus:ring-rayhunter-blue"
                            />
                            <p class="text-xs text-gray-500 mt-1">
                                How long a recording must be closed before it becomes eligible for
                                upload.
                            </p>
                        </div>

                        <div class="flex items-center">
                            <input
                                id="webdav_delete_on_upload"
                                type="checkbox"
                                bind:checked={config.webdav.delete_on_upload}
                                class="h-4 w-4 text-rayhunter-blue focus:ring-rayhunter-blue border-gray-300 rounded-sm"
                            />
                            <label
                                for="webdav_delete_on_upload"
                                class="ml-2 block text-sm text-gray-700"
                            >
                                Delete on successful upload
                            </label>
                        </div>
                        <p class="text-xs text-gray-500">
                            When enabled, the local files are removed after a successful upload.
                            Otherwise the manifest is just marked as uploaded.
                        </p>
                    </ExpandableInput>
                </div>

                {#if config.device === 'orbic' || config.device === 'moxee' || config.device === 'tmobile' || config.device === 'wingtech'}
                    <div class="border-t border-gray-200 pt-4 mt-6 space-y-3">
                        <h3 class="text-lg font-semibold text-gray-800 mb-4">WiFi Client Mode</h3>
                        <p class="text-xs text-gray-500">
                            Connect the device to an existing WiFi network for internet access (e.g.
                            notifications, remote access). The hotspot AP stays running alongside
                            WiFi client mode.
                        </p>

                        <div class="flex items-center">
                            <input
                                id="wifi_enabled"
                                type="checkbox"
                                bind:checked={config.wifi_enabled}
                                class="h-4 w-4 text-rayhunter-blue focus:ring-rayhunter-blue border-gray-300 rounded-sm"
                            />
                            <label for="wifi_enabled" class="ml-2 block text-sm text-gray-700">
                                Enable WiFi
                            </label>
                        </div>
                        <p class="text-xs text-gray-500">
                            Unchecking stops WiFi without clearing saved credentials.
                        </p>

                        {#if wifiStatus && config.wifi_enabled}
                            {#if wifiStatus.state === 'connected'}
                                <p class="text-xs text-green-600">
                                    Connected to "{wifiStatus.ssid}" ({wifiStatus.ip})
                                </p>
                            {:else if wifiStatus.state === 'connecting'}
                                <p class="text-xs text-amber-600">Connecting...</p>
                            {:else if wifiStatus.state === 'recovering'}
                                <p class="text-xs text-amber-600">Recovering connection...</p>
                            {:else if wifiStatus.state === 'dataPathDead'}
                                <p class="text-xs text-amber-600">
                                    Data path stalled, attempting recovery...
                                </p>
                            {:else if wifiStatus.state === 'failed'}
                                <p class="text-xs text-red-600">
                                    Failed: {wifiStatus.error}
                                </p>
                            {/if}
                        {/if}

                        <div>
                            <label
                                for="wifi_ssid"
                                class="block text-sm font-medium text-gray-700 mb-1"
                            >
                                WiFi Network Name (SSID)
                            </label>
                            <div class="flex gap-2">
                                <input
                                    id="wifi_ssid"
                                    type="text"
                                    bind:value={config.wifi_ssid}
                                    placeholder="MyWiFiNetwork"
                                    class="flex-1 px-3 py-2 border border-gray-300 rounded-md focus:outline-hidden focus:ring-2 focus:ring-rayhunter-blue"
                                />
                                <button
                                    type="button"
                                    onclick={do_scan}
                                    disabled={scanning}
                                    class="px-3 py-2 text-sm bg-gray-100 hover:bg-gray-200 disabled:opacity-50 border border-gray-300 rounded-md"
                                >
                                    {scanning ? 'Scanning...' : 'Scan'}
                                </button>
                            </div>
                        </div>

                        {#if scanError}
                            <p class="text-xs text-red-600">{scanError}</p>
                        {/if}

                        {#if scanResults.length > 0}
                            <div
                                class="border border-gray-200 rounded-md max-h-40 overflow-y-auto divide-y divide-gray-200"
                            >
                                {#each scanResults as network}
                                    <button
                                        type="button"
                                        class="w-full px-3 py-2 text-left text-sm hover:bg-gray-50 flex justify-between"
                                        onclick={() => select_network(network)}
                                    >
                                        <span>{network.ssid}</span>
                                        <span class="text-gray-400"
                                            >{network.signal_dbm} dBm &middot; {network.security}</span
                                        >
                                    </button>
                                {/each}
                            </div>
                        {/if}

                        {#if config.wifi_ssid}
                            <div>
                                <label
                                    for="wifi_security"
                                    class="block text-sm font-medium text-gray-700 mb-1"
                                >
                                    Security Type
                                </label>
                                <select
                                    id="wifi_security"
                                    bind:value={config.wifi_security}
                                    class="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-hidden focus:ring-2 focus:ring-rayhunter-blue"
                                >
                                    <option value="wpa_psk">WPA2 (WPA-PSK)</option>
                                    <option value="sae">WPA3 (SAE)</option>
                                </select>
                            </div>
                        {/if}

                        <div>
                            <label
                                for="wifi_password"
                                class="block text-sm font-medium text-gray-700 mb-1"
                            >
                                WiFi Password
                            </label>
                            <input
                                id="wifi_password"
                                type="password"
                                bind:value={config.wifi_password}
                                placeholder="Enter password"
                                class="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-hidden focus:ring-2 focus:ring-rayhunter-blue"
                            />
                            <p class="text-xs text-gray-500 mt-1">
                                Changing the network requires re-entering the password.
                            </p>
                        </div>

                        {#if config.wifi_ssid}
                            <div>
                                <label
                                    for="dns_servers"
                                    class="block text-sm font-medium text-gray-700 mb-1"
                                >
                                    DNS Servers
                                </label>
                                <input
                                    id="dns_servers"
                                    type="text"
                                    bind:value={dnsServersInput}
                                    placeholder="9.9.9.9, 149.112.112.112"
                                    class="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-hidden focus:ring-2 focus:ring-rayhunter-blue"
                                />
                                <p class="text-xs text-gray-500 mt-1">
                                    Comma-separated. Used when WiFi is active. Defaults to 9.9.9.9,
                                    149.112.112.112 (Quad9).
                                </p>
                            </div>
                        {/if}
                    </div>
                {/if}

                <div class="border-t border-gray-200 pt-4 mt-6">
                    <h3 class="text-lg font-semibold text-gray-800 mb-4">
                        Analyzer Heuristic Settings
                    </h3>
                    <div class="space-y-3">
                        <div class="flex items-center">
                            <input
                                id="imsi_requested"
                                type="checkbox"
                                bind:checked={config.analyzers.imsi_requested}
                                class="h-4 w-4 text-rayhunter-blue focus:ring-rayhunter-blue border-gray-300 rounded-sm"
                            />
                            <label for="imsi_requested" class="ml-2 block text-sm text-gray-700">
                                IMSI Requested Heuristic
                            </label>
                        </div>

                        <div class="flex items-center">
                            <input
                                id="connection_redirect_2g_downgrade"
                                type="checkbox"
                                bind:checked={config.analyzers.connection_redirect_2g_downgrade}
                                class="h-4 w-4 text-rayhunter-blue focus:ring-rayhunter-blue border-gray-300 rounded-sm"
                            />
                            <label
                                for="connection_redirect_2g_downgrade"
                                class="ml-2 block text-sm text-gray-700"
                            >
                                Connection Redirect 2G Downgrade Heuristic
                            </label>
                        </div>

                        <div class="flex items-center">
                            <input
                                id="lte_sib6_and_7_downgrade"
                                type="checkbox"
                                bind:checked={config.analyzers.lte_sib6_and_7_downgrade}
                                class="h-4 w-4 text-rayhunter-blue focus:ring-rayhunter-blue border-gray-300 rounded-sm"
                            />
                            <label
                                for="lte_sib6_and_7_downgrade"
                                class="ml-2 block text-sm text-gray-700"
                            >
                                LTE SIB6 and SIB7 Downgrade Heuristic
                            </label>
                        </div>

                        <div class="flex items-center">
                            <input
                                id="null_cipher"
                                type="checkbox"
                                bind:checked={config.analyzers.null_cipher}
                                class="h-4 w-4 text-rayhunter-blue focus:ring-rayhunter-blue border-gray-300 rounded-sm"
                            />
                            <label for="null_cipher" class="ml-2 block text-sm text-gray-700">
                                Null Cipher Heuristic
                            </label>
                        </div>

                        <div class="flex items-center">
                            <input
                                id="nas_null_cipher"
                                type="checkbox"
                                bind:checked={config.analyzers.nas_null_cipher}
                                class="h-4 w-4 text-rayhunter-blue focus:ring-rayhunter-blue border-gray-300 rounded-sm"
                            />
                            <label for="nas_null_cipher" class="ml-2 block text-sm text-gray-700">
                                NAS Null Cipher Heuristic
                            </label>
                        </div>

                        <div class="flex items-center">
                            <input
                                id="incomplete_sib"
                                type="checkbox"
                                bind:checked={config.analyzers.incomplete_sib}
                                class="h-4 w-4 text-rayhunter-blue focus:ring-rayhunter-blue border-gray-300 rounded-sm"
                            />
                            <label for="incomplete_sib" class="ml-2 block text-sm text-gray-700">
                                Incomplete SIB Heuristic
                            </label>
                        </div>

                        <div class="flex items-center">
                            <input
                                id="test_analyzer"
                                type="checkbox"
                                bind:checked={config.analyzers.test_analyzer}
                                class="h-4 w-4 text-rayhunter-blue focus:ring-rayhunter-blue border-gray-300 rounded-sm"
                            />
                            <label for="test_analyzer" class="ml-2 block text-sm text-gray-700">
                                Test Heuristic (noisy!)
                            </label>
                        </div>
                        <div class="flex items-center">
                            <input
                                id="diagnostic_analyzer"
                                type="checkbox"
                                bind:checked={config.analyzers.diagnostic_analyzer}
                                class="h-4 w-4 text-rayhunter-blue focus:ring-rayhunter-blue border-gray-300 rounded-sm"
                            />
                            <label
                                for="diagnostic_analyzer"
                                class="ml-2 block text-sm text-gray-700"
                            >
                                Diagnostic Analyzer
                            </label>
                        </div>
                    </div>
                </div>

                <div class="border-t border-gray-200 pt-4 mt-6 space-y-3">
                    <h3 class="text-lg font-semibold text-gray-800 mb-4">GPS Settings</h3>
                    <div>
                        <label for="gps_mode" class="block text-sm font-medium text-gray-700 mb-1"
                            >GPS Mode</label
                        >
                        <select
                            id="gps_mode"
                            bind:value={config.gps_mode}
                            class="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-rayhunter-blue"
                        >
                            <option value={GpsMode.Disabled}>Disabled</option>
                            <option value={GpsMode.Fixed}>Fixed coordinates</option>
                            <option value={GpsMode.Api}>API endpoint</option>
                        </select>
                        <p class="text-xs text-gray-500 mt-1">
                            {#if config.gps_mode === GpsMode.Api}
                                POST latitude and longitude to <code>/api/gps</code> from any device on
                                the network. Timestamp is derived from packet capture timing.
                            {:else if config.gps_mode === GpsMode.Fixed}
                                GPS coordinates are fixed to the values below.
                            {:else}
                                GPS is disabled; no coordinates will be tracked.
                            {/if}
                        </p>
                    </div>
                    {#if config.gps_mode === GpsMode.Fixed}
                        <div>
                            <label
                                for="gps_fixed_latitude"
                                class="block text-sm font-medium text-gray-700 mb-1"
                                >Fixed Latitude</label
                            >
                            <input
                                id="gps_fixed_latitude"
                                type="number"
                                min="-90"
                                max="90"
                                step="any"
                                required
                                bind:value={config.gps_fixed_latitude}
                                placeholder="e.g. 37.7749"
                                class="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-rayhunter-blue"
                            />
                            <p class="text-xs text-gray-500 mt-1">Decimal degrees, -90 to 90</p>
                        </div>
                        <div>
                            <label
                                for="gps_fixed_longitude"
                                class="block text-sm font-medium text-gray-700 mb-1"
                                >Fixed Longitude</label
                            >
                            <input
                                id="gps_fixed_longitude"
                                type="number"
                                min="-180"
                                max="180"
                                step="any"
                                required
                                bind:value={config.gps_fixed_longitude}
                                placeholder="e.g. -122.4194"
                                class="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-rayhunter-blue"
                            />
                            <p class="text-xs text-gray-500 mt-1">Decimal degrees, -180 to 180</p>
                        </div>
                    {/if}
                </div>

                <div class="flex gap-2 pt-4">
                    <button
                        type="submit"
                        disabled={saving}
                        class="bg-blue-500 hover:bg-blue-700 disabled:opacity-50 text-white font-bold py-2 px-4 rounded-md flex flex-row gap-1 items-center"
                    >
                        {#if saving}
                            <div
                                class="w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin"
                            ></div>
                            Saving...
                        {:else}
                            <svg
                                class="w-4 h-4"
                                fill="none"
                                stroke="currentColor"
                                viewBox="0 0 24 24"
                            >
                                <path
                                    stroke-linecap="round"
                                    stroke-linejoin="round"
                                    stroke-width="2"
                                    d="M5 13l4 4L19 7"
                                ></path>
                            </svg>
                            Apply and restart
                        {/if}
                    </button>
                </div>
            </form>
            {#if message}
                <div
                    class="mt-4 p-3 rounded-sm {messageType === 'error'
                        ? 'bg-red-100 text-red-700'
                        : 'bg-green-100 text-green-700'}"
                >
                    {message}
                </div>
            {/if}
        {:else}
            <div class="text-center py-4 text-red-600">
                Failed to load configuration. Please try reloading the page.
            </div>
        {/if}
    </div>
</Modal>
