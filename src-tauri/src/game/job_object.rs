use crate::error::CoreError;
use std::process::Child;

/// The launcher prevents a normal close during gameplay. This also prevents an orphan
/// process from outliving its instance lease if the launcher crashes or is force-killed.
pub struct ProcessGroup {
    #[cfg(windows)]
    handle: isize,
}
impl ProcessGroup {
    pub fn attach(child: &mut Child) -> Result<Self, CoreError> {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::{Foundation::CloseHandle, System::JobObjects::*};
            // Every raw handle is owned here and closed exactly once; Windows copies the limit struct.
            unsafe {
                let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if handle.is_null() {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(CoreError::LaunchFailed);
                }
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                if SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const _,
                    std::mem::size_of_val(&info) as u32,
                ) == 0
                    || AssignProcessToJobObject(handle, child.as_raw_handle()) == 0
                {
                    CloseHandle(handle);
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(CoreError::LaunchFailed);
                }
                Ok(Self {
                    handle: handle as isize,
                })
            }
        }
        #[cfg(not(windows))]
        {
            let _ = child;
            Err(CoreError::UnsupportedVersion)
        }
    }
}
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.handle as _);
        }
    }
}
