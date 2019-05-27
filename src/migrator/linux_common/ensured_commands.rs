use crate::{
    common::{call, CmdRes, MigError, MigErrorKind},
    linux_common::whereis,
};
use log::{error, trace};
use std::collections::HashMap;

pub const DF_CMD: &str = "df";
pub const LSBLK_CMD: &str = "lsblk";
pub const FDISK_CMD: &str = "fdisk";
pub const FILE_CMD: &str = "file";
pub const UNAME_CMD: &str = "uname";
pub const MOUNT_CMD: &str = "mount";
pub const MOKUTIL_CMD: &str = "mokutil";
pub const GRUB_UPDT_CMD: &str = "update-grub";
pub const GRUB_REBOOT_CMD: &str = "grub-reboot";
pub const REBOOT_CMD: &str = "reboot";
pub const CHMOD_CMD: &str = "chmod";
pub const DD_CMD: &str = "dd";
pub const PARTPROBE_CMD: &str = "partprobe";
pub const GZIP_CMD: &str = "gzip";
pub const MKTEMP_CMD: &str = "mktemp";

#[derive(Debug)]
pub(crate) struct EnsuredCommands {
    cmd_table: HashMap<String, String>,
}

impl EnsuredCommands {
    pub fn new(cmds: &[&str]) -> Result<EnsuredCommands, MigError> {
        let mut cmd_table: HashMap<String, String> = HashMap::new();

        for cmd in cmds {
            if let Ok(cmd_path) = whereis(cmd) {
                cmd_table.insert(String::from(*cmd), cmd_path.clone());
            } else {
                let message = format!("cannot find required command {}", cmd);
                error!("{}", message);
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    &format!("{}", message),
                ));
            }
        }
        Ok(EnsuredCommands { cmd_table })
    }

    pub fn ensure_cmds(&mut self, cmds: &[&str]) -> Result<(), MigError> {
        for cmd in cmds {
            if let Ok(cmd_path) = whereis(cmd) {
                self.cmd_table.insert(String::from(*cmd), cmd_path.clone());
            } else {
                let message = format!("cannot find required command {}", cmd);
                error!("{}", message);
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    &format!("{}", message),
                ));
            }
        }
        Ok(())
    }

    pub fn ensure_cmd(&mut self, cmd: &str) -> Result<String, MigError> {
        if let Ok(cmd_path) = whereis(cmd) {
            self.cmd_table.insert(String::from(cmd), cmd_path.clone());
            Ok(cmd_path)
        } else {
            let message = format!("cannot find required command {}", cmd);
            error!("{}", message);
            Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!("{}", message),
            ))
        }
    }

    pub fn get_cmd<'a>(&'a self, cmd: &str) -> Result<&'a str, MigError> {
        if let Some(cmd_path) = self.cmd_table.get(cmd) {
            Ok(cmd_path)
        } else {
            Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!("The command is not a checked command: {}", cmd),
            ))
        }
    }

    pub fn call_cmd(
        &self,
        cmd: &str,
        args: &[&str],
        trim_stdout: bool,
    ) -> Result<CmdRes, MigError> {
        trace!(
            "call_cmd: entered with cmd: '{}', args: {:?}, trim: {}",
            cmd,
            args,
            trim_stdout
        );

        Ok(call(self.get_cmd(cmd)?, args, trim_stdout)?)
    }
}