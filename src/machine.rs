//! A VMM machine.

use std::{
    path::{Path, PathBuf},
    process::Stdio, time::Duration,
};

use crate::{
    config::{Config, JailerMode},
    Error,
};
use tokio::{
    fs::copy,
    process::{Child, Command}, time::sleep,
};
use uuid::Uuid;

use hyper::{Body, Client, Method, Request};
use hyperlocal::{UnixClientExt, UnixConnector, Uri};

// FIXME: Hardcoding for now. This should come from ChrootStrategy enum, when we've that.
const KERNEL_IMAGE_FILENAME: &'static str = "kernel";

/// A VMM machine.
#[derive(Debug)]
pub struct Machine<'m> {
    config: Config<'m>,
    child: Child,
    client: Client<UnixConnector>,
}

impl<'m> Machine<'m> {
    /// Create a new machine.
    ///
    /// The machine is not started yet.
    pub async fn new(mut config: Config<'m>) -> Result<Machine<'m>, Error> {
        if config.vm_id == None {
            config.vm_id = Some(Uuid::new_v4());
        }

        // TOOD: Validate other parts of config, e.g paths.

        // FIXME: Assuming jailer for now.
        let jailer = config.jailer_cfg.as_mut().expect("no jailer config");
        let (daemonize_arg, stdin, stdout, stderr) = match &mut jailer.mode {
            JailerMode::Daemon => ("--daemonize", Stdio::null(), Stdio::null(), Stdio::null()),
            JailerMode::Attached(stdio) => (
                "",
                stdio.stdin.take().unwrap_or_else(|| Stdio::piped()),
                stdio.stdout.take().unwrap_or_else(|| Stdio::piped()),
                stdio.stderr.take().unwrap_or_else(|| Stdio::piped()),
            ),
        };

        // Assemble the path to the jailed root folder on the host.
        let exec_file_base = jailer
            .exec_file
            .file_name()
            .ok_or_else(|| Error::InvalidJailerExecPath)?;
        let id_str = jailer.id.to_string();
        let rootfs = jailer
            .chroot_base_dir
            .join(exec_file_base)
            .join(&id_str)
            .join("root");

        // Copy the kernel image to the rootfs.
        copy(config.kernel_image_path, rootfs.join(KERNEL_IMAGE_FILENAME)).await?;
        // Now the initrd, if specified.
        config.initrd_path = match config.initrd_path {
            Some(initrd_path) => {
                let initrd_filename = initrd_path
                    .file_name()
                    .ok_or_else(|| Error::InvalidInitrdPath)?
                    .to_owned();
                copy(initrd_path.as_os_str(), rootfs.join(&initrd_filename)).await?;

                Some(PathBuf::from(initrd_filename).into())
            }
            None => None,
        };

        // Copy all drives to the rootfs.
        for drive in &mut config.drives {
            let drive_filename = drive
                .path_on_host
                .file_name()
                .ok_or_else(|| Error::InvalidDrivePath)?;
            copy(&drive.path_on_host, rootfs.join(drive_filename)).await?;

            drive.path_on_host = PathBuf::from(drive_filename).into();
        }

        config.kernel_image_path = Path::new(KERNEL_IMAGE_FILENAME).into();

        // Adjust socket file path.
        let socket_path = config.socket_path;
        config.socket_path = rootfs.join(&socket_path).into();

        // TODO: Handle fifos. See https://github.com/firecracker-microvm/firecracker-go-sdk/blob/f0a967ef386caec37f6533dce5797038edf8c226/jailer.go#L435

        let child = Command::new(jailer.jailer_binary.as_os_str())
            .args(&[
                "--id",
                &id_str,
                "--exec-file",
                jailer
                    .exec_file
                    .to_str()
                    .ok_or_else(|| Error::InvalidJailerExecPath)?,
                "--uid",
                &jailer.uid.to_string(),
                "--gid",
                &jailer.gid.to_string(),
                "--chroot-base-dir",
                jailer
                    .chroot_base_dir
                    .to_str()
                    .ok_or_else(|| Error::InvalidChrootBasePath)?,
                daemonize_arg,
                // `firecracker` binary args.
                "--",
                "--socket",
                socket_path
                    .to_str()
                    .ok_or_else(|| Error::InvalidSocketPath)?,
            ])
            .stdin(stdin)
            .stdout(stdout)
            .stderr(stderr)
            .spawn()?;

        // Give some time to the jailer to start up and create the socket.
        // FIXME: We should monitor the socket instead?
        sleep(Duration::from_secs(1)).await;

        // `request` doesn't provide API to connect to unix sockets so we we use the low-level
        // approach using hyper: https://github.com/seanmonstar/reqwest/issues/39
        let client = Client::unix();

        Ok(Self {
            config,
            child,
            client,
        })
    }

    /// Start the machine.
    pub async fn start(&mut self) -> Result<(), Error> {
        unimplemented!();
    }

    /// Stop the machine.
    pub async fn stop(&mut self) -> Result<(), Error> {
        unimplemented!();
    }

    /// Shutdown requests a clean shutdown of the VM by sending CtrlAltDelete on the virtual keyboard.
    pub async fn shutdown(&mut self) -> Result<(), Error> {
        unimplemented!();
    }
}
