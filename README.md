# Thelio Io Windows Driver

A Windows service that controls the fans on a [System76 Thelio](https://system76.com/desktops)
desktop. System76's firmware leaves fan control to the operating system; on Linux
this is handled by `system76-power`, but on Windows nothing drives the fans unless
this service is installed. It talks to the **Thelio Io** daughterboard over USB and
adjusts fan speed based on CPU/GPU temperature.

## Supported computers

This driver only works on a genuine System76 Thelio. It selects a fan curve based on
the model family reported by the firmware, so all current AMD Thelio Mira revisions
are supported, including **thelio-mira-r4-n3** and newer. The Thelio Major, Mega, and
Massive families are supported as well.

If a brand-new model isn't recognized yet, the service falls back to the standard fan
curve and keeps running rather than failing.

## Installing (for most people)

1. Download the latest `thelio-io-<version>-x86_64.msi` from the
   [**Releases**](https://github.com/cbeard87/thelio-io-windows/releases) page.
2. Double-click the `.msi` file.
3. The installer is not code-signed, so Windows SmartScreen may show a blue
   *"Windows protected your PC"* warning. Click **More info**, then **Run anyway**.
4. Follow the prompts. When it finishes, the **System76 Thelio Io** service is
   installed and starts automatically. Your fans are now under temperature control.

The service starts on every boot; there is nothing to launch manually.

### Confirming it's working

- Open **Services** (press `Win+R`, type `services.msc`, press Enter) and look for
  **System76 Thelio Io** — its status should be *Running*.
- Under load, your fans should ramp up and down with temperature.

### Uninstalling

Open **Settings → Apps → Installed apps** (or the classic *Add/Remove Programs*),
find **System76 Thelio Io**, and choose **Uninstall**. This stops and removes the
service.

### Troubleshooting

Logs are written to the Windows Event Log. Open **Event Viewer**, go to
**Windows Logs → Application**, and filter by the source **System76 Thelio Io**.
Common messages:

- *"failed to find any Thelio Io devices"* — Windows didn't detect the Thelio Io
  board. Check that you're on a Thelio and reboot; the service retries on the next
  start. (This driver already works around the Windows 11 `usbser.sys` issue where
  the board enumerates without a USB ID.)
- *"unsupported sys_vendor / product_version"* — this isn't a recognized System76
  Thelio.

## Building from source (for developers)

Prerequisites (install in this order):

1. [Git LFS](https://git-lfs.com/) — install **before** cloning this repository.
2. [Rust](https://rustup.rs/) — the pinned toolchain in `rust-toolchain` is installed
   automatically on first build.
3. [Chocolatey](https://chocolatey.org/install).
4. From an **Administrator** Command Prompt:
   ```
   choco install dotnet-8.0-sdk python3 wixtoolset
   ```
5. From a normal Command Prompt:
   ```
   cargo install cargo-wix
   ```

Build the installer:

```
python build.py
```

This compiles the Rust service (`thelio-io.exe`), publishes the .NET 8 sensor helper
(`thelio-io_wrapper.exe`, a self-contained single executable that reads temperatures
via [LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor)),
and packages everything into `target/wix/thelio-io-<version>-x86_64.msi`.

Release builds are produced automatically by CI (see `.github/workflows/ci.yml`) and
attached to GitHub Releases. Pass `--sign` to `build.py` to code-sign the MSI with an
SSL.com cloud signing key (requires the `SSL_COM_*` environment variables).
