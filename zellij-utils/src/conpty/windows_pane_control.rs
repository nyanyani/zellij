use anyhow::{Context, Result};
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Storage::FileSystem::WriteFile;
use windows_sys::Win32::System::Console::{GenerateConsoleCtrlEvent, CTRL_C_EVENT};
use windows_sys::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

#[derive(Clone, Default)]
pub struct WindowsPaneControlAdapter;

#[derive(Clone, Copy, Debug, Default)]
pub struct PaneControlTarget {
    pub pid: Option<u32>,
    pub input_write_handle: Option<HANDLE>,
}

impl PaneControlTarget {
    pub fn for_process(pid: u32) -> Self {
        Self {
            pid: Some(pid),
            input_write_handle: None,
        }
    }

    pub fn for_input(input_write_handle: HANDLE) -> Self {
        Self {
            pid: None,
            input_write_handle: Some(input_write_handle),
        }
    }
}

impl WindowsPaneControlAdapter {
    pub fn new() -> Self {
        Self
    }

    pub fn interrupt_pane(&self, target: &PaneControlTarget) -> Result<()> {
        let pid = target
            .pid
            .context("missing process id for pane interrupt")?;
        unsafe {
            let ok = GenerateConsoleCtrlEvent(CTRL_C_EVENT, pid);
            if ok == 0 {
                self.terminate_process(pid)
            } else {
                Ok(())
            }
        }
    }

    pub fn terminate_pane(&self, target: &PaneControlTarget) -> Result<()> {
        let pid = target
            .pid
            .context("missing process id for pane termination")?;
        self.terminate_process(pid)
            .with_context(|| format!("failed to terminate pid {pid}"))
    }

    pub fn force_terminate_pane(&self, target: &PaneControlTarget) -> Result<()> {
        let pid = target
            .pid
            .context("missing process id for forced pane termination")?;
        self.terminate_process(pid)
            .with_context(|| format!("failed to force-terminate pid {pid}"))
    }

    pub fn write_input(&self, target: &PaneControlTarget, buf: &[u8]) -> Result<usize> {
        let input_write_handle = target
            .input_write_handle
            .context("missing input handle for pane write")?;
        let mut written: u32 = 0;
        let ok = unsafe {
            WriteFile(
                input_write_handle,
                buf.as_ptr(),
                buf.len() as u32,
                &mut written,
                std::ptr::null_mut(),
            )
        };
        if ok != 0 {
            Ok(written as usize)
        } else {
            Err(std::io::Error::last_os_error()).context("WriteFile failed")
        }
    }

    pub fn kill(&self, pid: u32) -> Result<()> {
        self.terminate_pane(&PaneControlTarget::for_process(pid))
    }

    pub fn force_kill(&self, pid: u32) -> Result<()> {
        self.force_terminate_pane(&PaneControlTarget::for_process(pid))
    }

    pub fn send_sigint(&self, pid: u32) -> Result<()> {
        self.interrupt_pane(&PaneControlTarget::for_process(pid))
    }

    pub fn write_to_tty_stdin(&self, input_write_handle: HANDLE, buf: &[u8]) -> Result<usize> {
        self.write_input(&PaneControlTarget::for_input(input_write_handle), buf)
    }

    fn terminate_process(&self, pid: u32) -> Result<()> {
        unsafe {
            let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
            if handle == (0 as HANDLE) {
                return Err(std::io::Error::last_os_error()).context("OpenProcess failed");
            }
            let ok = TerminateProcess(handle, 1);
            let _ = windows_sys::Win32::Foundation::CloseHandle(handle);
            if ok == 0 {
                return Err(std::io::Error::last_os_error()).context("TerminateProcess failed");
            }
        }
        Ok(())
    }
}
