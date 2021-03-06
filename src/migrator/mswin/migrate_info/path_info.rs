use lazy_static::lazy_static;
use log::warn;
use regex::Regex;
use std::path::{Path, PathBuf};

use crate::{
    common::{MigError, MigErrorKind},
    defs::DISK_BY_PARTUUID_PATH,
    mswin::{
        msw_defs::FileSystem,
        util::to_linux_path,
        wmi_utils::{physical_drive::DriveType, LogicalDrive, Partition, PhysicalDrive, Volume},
    },
};
//use log::{debug, error, info, trace};

// \\?\Volume{e907ceea-7513-4f34-a1d1-fee089d1dd4b}\
const PARTUUID_RE: &str =
    r#"\\\\\?\\Volume\{([0-9,a-f]{8}-[0-9,a-f]{4}-[0-9,a-f]{4}-[0-9,a-f]{4}-[0-9,a-f]{12})\}\\"#;

const DRIVE_SUFFIX: &[char] = &['a', 'b', 'c', 'd', 'e'];

#[derive(Debug, Clone)]
pub(crate) struct PathInfo {
    path: PathBuf,
    linux_drive: PathBuf,
    linux_part: PathBuf,
    part_uuid: Option<String>,
    part_size: u64,
    fs_size: u64,
    fs_free: u64,
    fs_compressed: bool,
    file_system: FileSystem,
    /*
    volume: Volume,
    partition: Partition,
    drive: PhysicalDrive,
    mount: LogicalDrive,
    */
}

impl<'a> PathInfo {
    pub fn new(
        path: &Path,
        volume: &Volume,
        drive: &PhysicalDrive,
        partition: &Partition,
        mount: &LogicalDrive,
    ) -> Result<PathInfo, MigError> {
        lazy_static! {
            static ref PARTUUID_REGEX: Regex = Regex::new(PARTUUID_RE).unwrap();
        }

        let (part_uuid, linux_drive, linux_part) =
            if let Some(captures) = PARTUUID_REGEX.captures(volume.get_device_id()) {
                let partuuid = String::from(captures.get(1).unwrap().as_str());
                let drive = match drive.get_drive_type() {
                    DriveType::Scsi => {
                        PathBuf::from(format!("/dev/sd{}", DRIVE_SUFFIX[drive.get_index()]))
                    }
                    DriveType::Ide => {
                        PathBuf::from(format!("/dev/hd{}", DRIVE_SUFFIX[drive.get_index()]))
                    }
                    DriveType::Other => {
                        return Err(MigError::from_remark(
                            MigErrorKind::NotImpl,
                            &format!(
                                "Cannot derive linux drive name from drive type {:?}",
                                drive.get_drive_type()
                            ),
                        ));
                    }
                };
                let part = PathBuf::from(format!("{}/{}", DISK_BY_PARTUUID_PATH, partuuid));
                (Some(partuuid), drive, part)
            } else {
                warn!(
                    "No Part UUID extracted for volume '{}'",
                    volume.get_device_id()
                );
                match drive.get_drive_type() {
                    DriveType::Scsi => {
                        let drive = format!("/dev/sd{}", DRIVE_SUFFIX[drive.get_index()]);
                        let part = format!("{}{}", drive, partition.get_part_index() + 1);
                        (None, PathBuf::from(drive), PathBuf::from(part))
                    }
                    DriveType::Ide => {
                        let drive = format!("/dev/hd{}", DRIVE_SUFFIX[drive.get_index()]);
                        let part = format!("{}{}", drive, partition.get_part_index() + 1);
                        (None, PathBuf::from(drive), PathBuf::from(part))
                    }
                    DriveType::Other => {
                        return Err(MigError::from_remark(
                            MigErrorKind::NotImpl,
                            &format!(
                                "Cannot derive linux drive name from drive type {:?}",
                                drive.get_drive_type()
                            ),
                        ));
                    }
                }
            };

        // TODO: extract information rather than copy
        Ok(PathInfo {
            path: PathBuf::from(path),
            linux_drive,
            linux_part,
            part_uuid,
            part_size: partition.get_size(),
            fs_size: mount.get_size(),
            fs_free: mount.get_free_space(),
            fs_compressed: mount.is_compressed(),
            file_system: mount.get_file_system().clone(),
            /*
            volume: volume.clone(),
            partition: partition.clone(),
            drive: drive.clone(),
            mount: mount.clone(),
            */
        })
    }

    pub fn get_linux_path(&self) -> PathBuf {
        to_linux_path(&self.path)
    }

    pub fn get_path(&'a self) -> &'a Path {
        &self.path
    }

    pub fn get_linux_part(&'a self) -> &'a Path {
        &self.linux_part
    }

    pub fn get_linux_drive(&'a self) -> &'a Path {
        &self.linux_drive
    }

    pub fn get_linux_fstype(&self) -> &'static str {
        self.file_system.to_linux_str()
    }

    pub fn get_partuuid(&'a self) -> Option<&'a str> {
        if let Some(ref partuuid) = self.part_uuid {
            Some(partuuid)
        } else {
            None
        }
    }
}
