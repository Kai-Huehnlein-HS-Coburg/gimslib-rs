use std::ops::Deref;

use windows::Win32::{
    Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0},
    System::Threading::{INFINITE, WaitForSingleObject},
};

pub struct Event {
    handle: HANDLE,
}

impl Event {
    /// Creates an event with a default security descriptor, automatic reset, and no name
    pub fn new(initially_signaled: bool) -> Result<Self, Box<dyn std::error::Error>> {
        let handle = unsafe {
            windows::Win32::System::Threading::CreateEventA(None, false, initially_signaled, None)
        }?;

        Ok(Event { handle })
    }

    pub fn wait(&self) -> Result<(), Box<dyn std::error::Error>> {
        let result = unsafe { WaitForSingleObject(self.handle, INFINITE) };
        if result != WAIT_OBJECT_0 {
            return Err(format!(
                "Error while waiting for Windows event handle: {:#01X}",
                result.0
            )
            .into());
        }

        Ok(())
    }
}

impl Deref for Event {
    type Target = HANDLE;

    fn deref(&self) -> &Self::Target {
        &self.handle
    }
}

impl Drop for Event {
    fn drop(&mut self) {
        // Why does Microsoft not understand how RAII works?
        unsafe { CloseHandle(self.handle) }.unwrap();
    }
}
