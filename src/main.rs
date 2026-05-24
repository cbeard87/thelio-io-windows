use log::{
    debug,
    error,
    warn,
};
use std::{
    env::current_exe,
    ffi::OsString,
    io::{
        self,
        BufRead,
        BufReader,
        Write,
    },
    process::{
        Child,
        Command,
        Stdio,
        exit,
    },
    thread::sleep,
    time::Duration,
};
use thelio_io::{
    fan::FanCurve,
    Io,
};
use windows_service::{
    define_windows_service,
    service::{
        ServiceControl,
        ServiceControlAccept,
        ServiceExitCode,
        ServiceState,
        ServiceStatus,
        ServiceType,
    },
    service_dispatcher,
    service_control_handler::{
        self,
        ServiceControlHandlerResult,
    },
};

fn driver_loop(curve: &FanCurve, ios: &mut [Io], wrapper: &mut Child) -> io::Result<()> {
    let mut wrapper_in = wrapper.stdin.take().unwrap();
    let mut wrapper_out = BufReader::new(wrapper.stdout.take().unwrap());

    loop {
        wrapper_in.write_all(b"\n")?;
        let mut line = String::new();
        wrapper_out.read_line(&mut line)?;

        let temp = line.trim().parse::<f64>().map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                err
            )
        })?;

        if let Some(duty) = curve.get_duty((temp * 100.0) as i16) {
            for io in ios.iter_mut() {
                for device in &["CPUF", "INTF"] {
                    io.set_duty(device, duty).map_err(|err| {
                        io::Error::new(
                            io::ErrorKind::Other,
                            err
                        )
                    })?;
                }
            }
        }

        sleep(Duration::new(1, 0));
    }
}

/// Open a serial port and confirm a Thelio Io is on the other end via the firmware
/// `IoREVISION` handshake. Returns the ready-to-use device, or `None` if the port
/// can't be opened or doesn't speak the Io protocol (so probing unrelated ports is
/// harmless).
fn open_thelio_io(port_name: &str) -> Option<Io> {
    let port = serialport::new(port_name, 115200)
        .timeout(Duration::from_millis(1))
        .open()
        .ok()?;

    let mut io = Io::new(port, 1000);
    match io.revision() {
        Ok(revision) => {
            debug!("Thelio Io at {} (revision {})", port_name, revision);
            if let Err(err) = io.reset() {
                error!("Thelio Io at {} failed to reset: {}", port_name, err);
                return None;
            }
            Some(io)
        },
        Err(_) => None,
    }
}

fn driver() -> io::Result<()> {
    let smbios = smbioslib::table_load_from_device()?;

    let sys_vendor = smbios.find_map(
        |sys: smbioslib::SMBiosSystemInformation| sys.manufacturer()
    ).unwrap_or(String::new());

    let product_version = smbios.find_map(
        |sys: smbioslib::SMBiosSystemInformation| sys.version()
    ).unwrap_or(String::new());

    // Match by model family (prefix) rather than an exact model string. System76's
    // SMBIOS product version is inconsistent across revisions (e.g. "thelio-mira-r4"
    // vs "thelio-mira-r4-n3"), so prefix matching keeps every current and future
    // revision of a family working without a code change. Curve assignments mirror
    // the Linux daemon (https://github.com/pop-os/system76-power/blob/master/src/fan.rs).
    let curve = if sys_vendor != "System76" {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("unsupported sys_vendor '{}'", sys_vendor)
        ));
    } else if product_version == "thelio-major-r1" {
        debug!("{} {} uses threadripper2 fan curve", sys_vendor, product_version);
        FanCurve::threadripper2()
    } else if product_version.starts_with("thelio-major")
           || product_version.starts_with("thelio-mega")
           || product_version.starts_with("thelio-astra") {
        debug!("{} {} uses hedt fan curve", sys_vendor, product_version);
        FanCurve::hedt()
    } else if product_version.starts_with("thelio-massive") {
        debug!("{} {} uses xeon fan curve", sys_vendor, product_version);
        FanCurve::xeon()
    } else if product_version.starts_with("thelio-mira") {
        debug!("{} {} uses standard fan curve", sys_vendor, product_version);
        FanCurve::standard()
    } else if product_version.starts_with("thelio") {
        // Unknown Thelio family: default to the standard curve so a new model still
        // runs (and cools) until an explicit curve can be assigned for it.
        warn!("{} {} is not explicitly supported; using standard fan curve", sys_vendor, product_version);
        FanCurve::standard()
    } else {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "unsupported sys_vendor '{}' and product_version '{}'",
                sys_vendor,
                product_version
            )
        ));
    };

    let ports = serialport::available_ports()?;
    let mut ios = Vec::new();

    // Pass 1: ports that advertise the Thelio Io USB VID/PID (the normal case).
    for port_info in &ports {
        if let serialport::SerialPortType::UsbPort(usb_info) = &port_info.port_type {
            if usb_info.vid == 0x1209 && usb_info.pid == 0x1776 {
                if let Some(io) = open_thelio_io(&port_info.port_name) {
                    ios.push(io);
                }
            }
        }
    }

    // Pass 2: fallback for Windows 11, where the generic usbser.sys driver makes
    // the port enumerate as `Unknown` (no VID/PID), so Pass 1 misses it. Probe each
    // unidentified port with the Io handshake and keep the ones that respond.
    if ios.is_empty() {
        for port_info in &ports {
            if matches!(port_info.port_type, serialport::SerialPortType::Unknown) {
                if let Some(io) = open_thelio_io(&port_info.port_name) {
                    ios.push(io);
                }
            }
        }
    }

    if ios.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "failed to find any Thelio Io devices"
        ));
    }

    let bin_path = current_exe()?;
    let bin_dir = bin_path.parent().unwrap();
    let wrapper_path = bin_dir.join("thelio-io_wrapper.exe");
    let mut wrapper = Command::new(&wrapper_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    let res = driver_loop(&curve, &mut ios, &mut wrapper);

    let _ = wrapper.kill();

    res
}

fn service_main(_args: Vec<OsString>) {
    // Windows event log
    winlog::init("System76 Thelio Io").expect("failed to initialize logging");

    // Handle service events
    let status_handle = service_control_handler::register("thelio-io", |event| -> ServiceControlHandlerResult {
        //TODO: handle stop event
        match event {
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    }).expect("failed to register for service events");

    // Update service status
    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    }).expect("failed to set service status");

    // Run driver
    if let Err(err) = driver() {
        error!("{}\n{:#?}", err, err);
        //TODO: set service status
        exit(1);
    }
}

define_windows_service!(ffi_service_main, service_main);

fn main() -> Result<(), windows_service::Error> {
    // Dispatch service
    service_dispatcher::start("thelio-io", ffi_service_main)?;
    Ok(())
}
