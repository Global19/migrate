use crate::{
    common::{
        call, device_info::DeviceInfo, file_exists, path_append, path_info::PathInfo, MigError,
        MigErrorKind,
    },
    mswin::{
        util::mount_efi,
        win_api::{get_volume_disk_extents, is_efi_boot, DiskExtent},
        wmi_utils::{
            volume::{DriveType, Volume},
            LogicalDrive, Partition, PhysicalDrive,
        },
    },
};
use log::debug;
use regex::{Captures, Regex};
use std::mem::{swap, transmute};
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, Once};

// \\?\Volume{345ad334-48a8-11e8-9eaf-806e6f6e6963}\

const VOL_UUID_REGEX: &str =
    r#"^\\\\\?\\Volume\{([a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12})\}\\$"#;

#[derive(Clone)]
pub(crate) struct VolumeInfo {
    pub part_uuid: String,
    pub volume: Volume,
    pub logical_drive: LogicalDrive,
    pub physical_drive: PhysicalDrive,
    pub partition: Partition,
}

impl VolumeInfo {}

struct SharedDriveInfo {
    volumes: Option<Vec<VolumeInfo>>,
}

#[derive(Clone)]
pub(crate) struct DriveInfo {
    inner: Arc<Mutex<SharedDriveInfo>>,
}

impl DriveInfo {
    pub fn new() -> Result<DriveInfo, MigError> {
        static mut DRIVE_INFO: *const DriveInfo = 0 as *const DriveInfo;
        static ONCE: Once = Once::new();

        let drive_info = unsafe {
            ONCE.call_once(|| {
                // Make it
                //dbg!("call_once");
                let singleton = DriveInfo {
                    inner: Arc::new(Mutex::new(SharedDriveInfo { volumes: None })),
                };

                // Put it in the heap so it can outlive this call
                DRIVE_INFO = transmute(Box::new(singleton));
            });

            (*DRIVE_INFO).clone()
        };

        {
            let _dummy = drive_info.init()?;
        }
        Ok(drive_info)
    }

    pub fn for_efi_drive(&self) -> Result<VolumeInfo, MigError> {
        let drive_info = self.init()?;
        //let volumes = ;
        if let Some(found) = drive_info
            .volumes
            .as_ref()
            .unwrap()
            .iter()
            .find(|vi| vi.volume.is_system())
        {
            Ok(found.clone())
        } else {
            Err(MigError::from_remark(
                MigErrorKind::NotFound,
                "Could not find EFI / System drive",
            ))
        }
    }

    pub fn from_path<P: AsRef<Path>>(&self, path: P) -> Result<VolumeInfo, MigError> {
        let drive_info = self.init()?;
        if let Some(found) = drive_info
            .volumes
            .as_ref()
            .unwrap()
            .iter()
            .find(|di| PathBuf::from(di.logical_drive.get_name()).starts_with(path.as_ref()))
        {
            Ok(found.clone())
        } else {
            Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!(
                    "No logical drive found for path '{}'",
                    path.as_ref().display()
                ),
            ))
        }
    }

    fn init(&self) -> Result<MutexGuard<SharedDriveInfo>, MigError> {
        let mut shared_di = self.inner.lock().unwrap();
        if shared_di.volumes.is_some() {
            return Ok(shared_di);
        }

        let mut efi_drive = if is_efi_boot()? {
            Some(mount_efi()?)
        } else {
            None
        };

        let mut vol_infos: Vec<VolumeInfo> = Vec::new();

        let part_uuid_re = Regex::new(VOL_UUID_REGEX).unwrap();

        let volumes = Volume::query_all()?;
        for volume in volumes {
            match volume.get_drive_type() {
                DriveType::LocalDisk | DriveType::RemovableDisk => (),
                _ => {
                    debug!(
                        "Unsupported drive type: {:?} for volume '{}', skipping volume",
                        volume.get_drive_type(),
                        volume.get_device_id()
                    );
                    continue;
                }
            }
            let logical_drive = if let Some(drive_letter) = volume.get_drive_letter() {
                LogicalDrive::query_for_name(drive_letter)?
            } else {
                if volume.is_system() {
                    let mut swapped_efi_drive: Option<LogicalDrive> = None;
                    swap(&mut swapped_efi_drive, &mut efi_drive);
                    if let Some(efi_drive) = swapped_efi_drive {
                        efi_drive
                    } else {
                        debug!(
                            "No logicalDrive found for volume '{}' - skipping volume",
                            volume.get_device_id()
                        );
                        continue;
                    }
                } else {
                    debug!(
                        "No logicalDrive found for volume '{}' - skipping volume",
                        volume.get_device_id()
                    );
                    continue;
                }
            };

            // get DiskPartition for volume
            let disk_extents =
                get_volume_disk_extents(&format!("\\\\.\\{}", logical_drive.get_name()))?;

            if disk_extents.len() == 1 {
                let disk_extent = &disk_extents[0];
                let physical_drive = PhysicalDrive::by_index(disk_extent.disk_index as usize)?;
                if let Some(partition) = physical_drive
                    .query_partitions()?
                    .iter()
                    .find(|part| part.get_start_offset() == disk_extent.start_offset as u64)
                {
                    // got volume & partition here

                    // Her we have all components: PhysicalDrive, Volume, Partition, LogicalDrive
                    vol_infos.push(VolumeInfo {
                        part_uuid: if let Some(captures) =
                            part_uuid_re.captures(volume.get_device_id())
                        {
                            String::from(captures.get(1).unwrap().as_str())
                        } else {
                            return Err(MigError::from_remark(
                                MigErrorKind::NoMatch,
                                &format!(
                                    "Could not extract partuuid from volume id: '{}'",
                                    volume.get_device_id()
                                ),
                            ));
                        },
                        volume,
                        physical_drive,
                        logical_drive: logical_drive.clone(),
                        partition: partition.clone(),
                    });
                } else {
                    return Err(MigError::from_remark(
                        MigErrorKind::NotFound,
                        &format!(
                            "Could not find partition for disk index: {}, start offset: {}",
                            disk_extent.disk_index, disk_extent.start_offset
                        ),
                    ));
                }
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "Encountered invalid number of disk extents ({} != 1) for volume '{}'",
                        disk_extents.len(),
                        volume.get_device_id()
                    ),
                ));
            }
        }

        shared_di.volumes = Some(vol_infos);
        Ok(shared_di)
    }
}
