//! Priority and CPU core choice for the Roblox game process.
//!
//! After Roblox starts, Windows is told to give it more CPU time (priority class) and, optionally, to keep it
//! off core 0 (the core Windows uses for most interrupts). Both are plain per-process settings: they last
//! until Roblox closes and need no admin rights.

use std::io;

use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::{
    GetProcessAffinityMask, OpenProcess, SetPriorityClass, SetProcessAffinityMask, ABOVE_NORMAL_PRIORITY_CLASS,
    HIGH_PRIORITY_CLASS, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_INFORMATION,
};

use crate::app;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Priority {
    Default,
    AboveNormal,
    High,
}

impl Priority {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "default" => Some(Self::Default),
            "above" => Some(Self::AboveNormal),
            "high" => Some(Self::High),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::AboveNormal => "above",
            Self::High => "high",
        }
    }

    pub(crate) fn class(self) -> Option<u32> {
        match self {
            Self::Default => None,
            Self::AboveNormal => Some(ABOVE_NORMAL_PRIORITY_CLASS),
            Self::High => Some(HIGH_PRIORITY_CLASS),
        }
    }
}

/// Applies the saved boost settings to a running Roblox process. Never fails the launch; problems go to the log.
pub fn apply(pid: u32) {
    const IDENT: &str = "Boost::Apply";
    let (prio, skip_core0) = {
        let s = app::settings();
        (Priority::parse(&s.prop.process_priority).unwrap_or(Priority::Default), s.prop.skip_core0)
    };
    if prio == Priority::Default && !skip_core0 {
        return;
    }

    // SAFETY: plain Win32 calls on a handle we open and close here.
    unsafe {
        let h = OpenProcess(PROCESS_SET_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            crate::log!(IDENT, "OpenProcess({pid}) failed: {}", io::Error::last_os_error());
            return;
        }

        if let Some(class) = prio.class() {
            if SetPriorityClass(h, class) != 0 {
                crate::log!(IDENT, "Priority '{}' set for PID {pid}", prio.as_str());
            } else {
                crate::log!(IDENT, "SetPriorityClass failed: {}", io::Error::last_os_error());
            }
        }

        if skip_core0 {
            let (mut proc_mask, mut sys_mask) = (0usize, 0usize);
            if GetProcessAffinityMask(h, &mut proc_mask, &mut sys_mask) == 0 {
                crate::log!(IDENT, "GetProcessAffinityMask failed: {}", io::Error::last_os_error());
            } else if sys_mask.count_ones() <= 2 {
                crate::log!(IDENT, "Only {} cores, keeping core 0", sys_mask.count_ones());
            } else {
                let new_mask = proc_mask & !1usize;
                if new_mask == 0 || new_mask == proc_mask {
                    crate::log!(IDENT, "Affinity already skips core 0 (or nothing left), no change");
                } else if SetProcessAffinityMask(h, new_mask) != 0 {
                    crate::log!(IDENT, "Affinity mask {proc_mask:#x} -> {new_mask:#x} for PID {pid}");
                } else {
                    crate::log!(IDENT, "SetProcessAffinityMask failed: {}", io::Error::last_os_error());
                }
            }
        }
        CloseHandle(h);
    }
}
