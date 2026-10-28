//! The client as a Windows service: listed in `services.msc` and Task Manager, started at boot, restarted after a failure, for every
//! user of the computer. Two sides: `run` is the service itself (the Service Control Manager starts `fafnirx service`, which
//! hands the process over), and the rest is how installing, starting and stopping it is asked of the Service Control Manager,
//! which any user may do for starting and stopping (see `install`) and only an administrator for the rest.

use anyhow::{anyhow, bail, Context, Result};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, OnceLock};
use std::time::{Duration, Instant};
use windows_service::service::{
    Service, ServiceAccess, ServiceAction, ServiceActionType, ServiceControl, ServiceControlAccept,
    ServiceErrorControl, ServiceExitCode, ServiceFailureActions, ServiceFailureResetPeriod, ServiceInfo,
    ServiceStartType, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult, ServiceStatusHandle};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
use windows_service::{define_windows_service, service_dispatcher};

pub const NAME: &str = "fafnirx";

/// Who may do what to the service: the system and the administrators everything, and every signed-in user ask where it is, start
/// it and stop it (so the tray and `fafnirx restart` need no administrator), but not change it.
const ACCESS: &str = "D:(A;;CCLCSWRPWPDTLOCRRC;;;SY)(A;;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;BA)(A;;CCLCSWRPWPLORC;;;AU)";

static CONFIG: OnceLock<PathBuf> = OnceLock::new();
static HANDLE: OnceLock<ServiceStatusHandle> = OnceLock::new();
static IN_SERVICE: AtomicBool = AtomicBool::new(false);

/// Whether this process is the service, which has to say so before it ends or the Service Control Manager takes it for a crash.
pub fn in_service() -> bool {
    IN_SERVICE.load(Ordering::SeqCst)
}

define_windows_service!(ffi_service_main, service_main);

/// `fafnirx service`: hands this process to the Service Control Manager, which runs the client in it until it is told to stop.
pub fn run(config: PathBuf) -> Result<()> {
    let _ = CONFIG.set(config);
    IN_SERVICE.store(true, Ordering::SeqCst);
    service_dispatcher::start(NAME, ffi_service_main)
        .context("this is run by the Service Control Manager: use `fafnirx run` to run the client by hand")
}

fn service_main(_: Vec<OsString>) {
    if let Err(e) = host() {
        tracing::error!("the service stopped: {e:#}");
        exit_with(1);
    }
}

fn report(state: ServiceState, code: u32, accepts: ServiceControlAccept) {
    if let Some(handle) = HANDLE.get() {
        let status = ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: state,
            controls_accepted: accepts,
            exit_code: ServiceExitCode::Win32(code),
            checkpoint: 0,
            wait_hint: Duration::from_secs(10),
            process_id: None,
        };
        let _ = handle.set_service_status(status);
    }
}

/// Says the service is stopped (with an exit code: not 0 makes the Service Control Manager start it again, see `install`) and
/// ends the process.
fn exit_with(code: u32) -> ! {
    report(ServiceState::Stopped, code, ServiceControlAccept::empty());
    std::process::exit(code as i32)
}

/// A stop the person asked for (the tray's "Stop client"): the service stays stopped.
pub fn exit_clean() -> ! {
    exit_with(0)
}

/// An exit to be started again, on a new version or a new enrollment.
pub fn exit_for_restart() -> ! {
    exit_with(1)
}

fn host() -> Result<()> {
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let handle = service_control_handler::register(NAME, move |control| match control {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            let _ = stop_tx.send(());
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    })?;
    let _ = HANDLE.set(handle);
    report(ServiceState::Running, 0, ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN);

    let config = CONFIG.get().cloned().ok_or_else(|| anyhow!("no config"))?;
    let (done_tx, done_rx) = mpsc::channel::<std::result::Result<(), String>>();
    std::thread::spawn(move || {
        let result = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .max_blocking_threads(8)
            .enable_all()
            .build()
            .map_err(|e| e.to_string())
            .and_then(|runtime| runtime.block_on(crate::daemon::run(config)).map_err(|e| format!("{e:#}")));
        let _ = done_tx.send(result);
    });
    loop {
        if stop_rx.recv_timeout(Duration::from_millis(500)).is_ok() {
            report(ServiceState::StopPending, 0, ServiceControlAccept::empty());
            exit_with(0);
        }
        match done_rx.try_recv() {
            Ok(Ok(())) => exit_with(0),
            Ok(Err(e)) => bail!("{e}"),
            // The client's thread ended without an answer: it panicked (the hook logged where). Ending with an error makes the
            // Service Control Manager start it again, instead of a service that runs and does nothing.
            Err(mpsc::TryRecvError::Disconnected) => bail!("the client stopped after an error (see above)"),
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }
}

// --- asking the Service Control Manager -------------------------------------------------------------------------------------------

fn manager(access: ServiceManagerAccess) -> Result<ServiceManager> {
    ServiceManager::local_computer(None::<&str>, access).context("connect to the Service Control Manager")
}

fn open(access: ServiceAccess) -> Result<Service> {
    manager(ServiceManagerAccess::CONNECT)?.open_service(NAME, access).context("the fafnirx service is not installed")
}

pub fn installed() -> bool {
    open(ServiceAccess::QUERY_STATUS).is_ok()
}

fn state(service: &Service) -> Result<ServiceState> {
    Ok(service.query_status()?.current_state)
}

fn wait_for(service: &Service, wanted: ServiceState) -> Result<()> {
    let started = Instant::now();
    while state(service)? != wanted {
        if started.elapsed() > Duration::from_secs(30) {
            bail!("the service did not reach the state {wanted:?} in 30 seconds");
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Ok(())
}

/// Starts the service; nothing to do when it runs.
pub fn start() -> Result<()> {
    let service = open(ServiceAccess::START | ServiceAccess::QUERY_STATUS)?;
    if state(&service)? != ServiceState::Running {
        service.start::<OsString>(&[]).context("start the service")?;
    }
    wait_for(&service, ServiceState::Running)
}

/// Stops the service and waits until it has; nothing to do when it is stopped.
pub fn stop() -> Result<()> {
    let service = open(ServiceAccess::STOP | ServiceAccess::QUERY_STATUS)?;
    if state(&service)? != ServiceState::Stopped {
        let _ = service.stop();
    }
    wait_for(&service, ServiceState::Stopped)
}

pub fn restart() -> Result<()> {
    stop()?;
    start()
}

/// Makes the service (an administrator does this): `exe --config <config> service`, started at boot, as the system account,
/// started again 5 s after it fails (and after 30 s the third time), also when it ends with an error of its own.
pub fn install(exe: &Path, config: &Path) -> Result<()> {
    let info = ServiceInfo {
        name: NAME.into(),
        display_name: "fafnirx backup client".into(),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: exe.to_path_buf(),
        launch_arguments: vec!["--config".into(), config.into(), "service".into()],
        dependencies: vec![],
        account_name: None,
        account_password: None,
    };
    let manager = manager(ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE)?;
    let access = ServiceAccess::CHANGE_CONFIG | ServiceAccess::START | ServiceAccess::QUERY_STATUS;
    let service = match manager.open_service(NAME, access) {
        Ok(existing) => {
            existing.change_config(&info).context("update the service")?;
            existing
        }
        Err(_) => manager.create_service(&info, access).context("create the service")?,
    };
    service.set_description("Backs up folders to a fafnirx master, and keeps shared folders in step.")?;
    let again =
        |seconds| ServiceAction { action_type: ServiceActionType::Restart, delay: Duration::from_secs(seconds) };
    service.update_failure_actions(ServiceFailureActions {
        reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(86_400)),
        reboot_msg: None,
        command: None,
        actions: Some(vec![again(5), again(5), again(30)]),
    })?;
    service.set_failure_actions_on_non_crash_failures(true)?;
    crate::service::run_quiet("sc", &["sdset", NAME, ACCESS])?;
    if state(&service)? == ServiceState::Stopped {
        service.start::<OsString>(&[]).context("start the service")?;
    }
    Ok(())
}

/// Stops and removes the service. What it backed up, and its config, stay.
pub fn uninstall() -> Result<()> {
    let service = open(ServiceAccess::STOP | ServiceAccess::QUERY_STATUS | ServiceAccess::DELETE)?;
    if state(&service)? != ServiceState::Stopped {
        let _ = service.stop();
        wait_for(&service, ServiceState::Stopped)?;
    }
    service.delete().context("remove the service")
}
