//! Windows 进程工具：按 pid 直接 TerminateProcess（不依赖 taskkill，秒杀）。

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn OpenProcess(
        desired_access: u32,
        inherit_handle: i32,
        process_id: u32,
    ) -> *mut std::ffi::c_void;
    fn TerminateProcess(process: *mut std::ffi::c_void, exit_code: u32) -> i32;
    fn CloseHandle(object: *mut std::ffi::c_void) -> i32;
}

const PROCESS_TERMINATE: u32 = 0x0001;
/// CREATE_NO_WINDOW：子进程不闪控制台窗口
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 直接强杀指定 pid（尽力而为：进程不存在/权限不足时静默失败）
#[cfg(windows)]
pub fn terminate_pid(pid: u32) {
    unsafe {
        let h = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if !h.is_null() {
            TerminateProcess(h, 1);
            CloseHandle(h);
        }
    }
}

#[cfg(not(windows))]
pub fn terminate_pid(_pid: u32) {}
