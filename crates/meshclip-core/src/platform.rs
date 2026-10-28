use anyhow::Result;
#[async_trait::async_trait]
pub trait NetworkAdapter: Send + Sync {
    async fn probe(&self, address: &str) -> Result<()>;
}
pub struct ExternalEasyTier;
#[async_trait::async_trait]
impl NetworkAdapter for ExternalEasyTier {
    async fn probe(&self, address: &str) -> Result<()> {
        let stream = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            tokio::net::TcpStream::connect(address),
        )
        .await??;
        drop(stream);
        Ok(())
    }
}
pub trait ClipboardPort {
    fn read_text(&mut self) -> Result<String>;
    fn write_text(&mut self, text: String) -> Result<()>;
    fn foreground_app(&self) -> Option<String>;
}
pub struct NativeClipboard(arboard::Clipboard);
impl NativeClipboard {
    pub fn open() -> Result<Self> {
        Ok(Self(arboard::Clipboard::new()?))
    }
}
impl ClipboardPort for NativeClipboard {
    fn read_text(&mut self) -> Result<String> {
        Ok(self.0.get_text()?)
    }
    fn write_text(&mut self, text: String) -> Result<()> {
        Ok(self.0.set_text(text)?)
    }
    fn foreground_app(&self) -> Option<String> {
        #[cfg(windows)]
        {
            foreground_process()
        }
        #[cfg(not(windows))]
        {
            None
        }
    }
}

#[cfg(windows)]
fn foreground_process() -> Option<String> {
    use std::ffi::c_void;
    #[link(name = "user32")]
    extern "system" {
        fn GetForegroundWindow() -> *mut c_void;
        fn GetWindowThreadProcessId(hwnd: *mut c_void, pid: *mut u32) -> u32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn QueryFullProcessImageNameW(
            process: *mut c_void,
            flags: u32,
            name: *mut u16,
            len: *mut u32,
        ) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }
    // Handles are inspected read-only, and always closed; limited query access requires no elevation.
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let process = OpenProcess(0x1000, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut buf = vec![0u16; 32768];
        let mut n = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, 0, buf.as_mut_ptr(), &mut n);
        CloseHandle(process);
        if ok == 0 {
            return None;
        }
        let path = String::from_utf16(&buf[..n as usize]).ok()?;
        Some(path.rsplit('\\').next()?.to_lowercase())
    }
}
