//! Names and short explanations for Windows stop codes and user-mode exception codes.
//!
//! Stop codes: Microsoft Learn, "Bug Check Code Reference"
//! (https://learn.microsoft.com/windows-hardware/drivers/debugger/bug-check-code-reference2).
//! Exception codes: ntstatus.h (Windows SDK) and the MSVC/CLR exception codes noted inline.
//! Descriptions are paraphrased summaries of the documented meaning. The "area" is
//! where the documentation says the code usually originates; it is a starting point
//! for investigation, never a diagnosis.

use serde::Serialize;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodeInfo {
    pub code: u32,
    pub name: &'static str,
    pub description: &'static str,
    pub area: &'static str,
}

const fn c(
    code: u32,
    name: &'static str,
    area: &'static str,
    description: &'static str,
) -> CodeInfo {
    CodeInfo {
        code,
        name,
        description,
        area,
    }
}

static BUGCHECKS: &[CodeInfo] = &[
    c(0x0A, "IRQL_NOT_LESS_OR_EQUAL", "Driver", "Kernel-mode code accessed paged or invalid memory at a raised interrupt request level. Usually a driver bug; occasionally faulty memory."),
    c(0x18, "REFERENCE_BY_POINTER", "Driver", "The reference count of an object is invalid for its current state."),
    c(0x19, "BAD_POOL_HEADER", "Memory / Driver", "A kernel pool header is corrupt."),
    c(0x1A, "MEMORY_MANAGEMENT", "Memory", "A severe memory-management error occurred. The first parameter identifies the subtype."),
    c(0x1E, "KMODE_EXCEPTION_NOT_HANDLED", "Driver", "A kernel-mode program raised an exception that no handler caught."),
    c(0x24, "NTFS_FILE_SYSTEM", "Storage", "A problem occurred in the NTFS file system driver."),
    c(0x3B, "SYSTEM_SERVICE_EXCEPTION", "Driver", "An exception occurred while executing a system service (transition from user to kernel mode)."),
    c(0x3D, "INTERRUPT_EXCEPTION_NOT_HANDLED", "Driver / Hardware", "An exception occurred during interrupt handling and was not handled."),
    c(0x44, "MULTIPLE_IRP_COMPLETE_REQUESTS", "Driver", "A driver tried to complete an I/O request packet that was already completed."),
    c(0x4E, "PFN_LIST_CORRUPT", "Memory / Driver", "The memory manager's page frame number list is corrupted."),
    c(0x50, "PAGE_FAULT_IN_NONPAGED_AREA", "Driver / Memory", "Invalid system memory was referenced."),
    c(0x51, "REGISTRY_ERROR", "Storage / System", "A severe registry error occurred."),
    c(0x5C, "HAL_INITIALIZATION_FAILED", "System", "Hardware abstraction layer initialization failed."),
    c(0x77, "KERNEL_STACK_INPAGE_ERROR", "Storage / Memory", "A page of kernel stack could not be read from the paging file."),
    c(0x7A, "KERNEL_DATA_INPAGE_ERROR", "Storage / Memory", "A page of kernel data could not be read from the paging file."),
    c(0x7B, "INACCESSIBLE_BOOT_DEVICE", "Storage", "Windows lost access to the system partition during startup."),
    c(0x7E, "SYSTEM_THREAD_EXCEPTION_NOT_HANDLED", "Driver", "A system thread raised an exception that no handler caught."),
    c(0x7F, "UNEXPECTED_KERNEL_MODE_TRAP", "Hardware / Driver", "The CPU raised a trap the kernel does not allow or catch, such as a double fault."),
    c(0x80, "NMI_HARDWARE_FAILURE", "Hardware", "A hardware non-maskable interrupt signalled a failure."),
    c(0x8E, "KERNEL_MODE_EXCEPTION_NOT_HANDLED", "Driver", "A kernel-mode application raised an exception that no handler caught."),
    c(0x9C, "MACHINE_CHECK_EXCEPTION", "Hardware", "A fatal machine check exception occurred (older systems; modern systems report 0x124)."),
    c(0x9F, "DRIVER_POWER_STATE_FAILURE", "Driver / Power", "A driver is in an inconsistent or invalid power state, often during sleep or resume."),
    c(0xA0, "INTERNAL_POWER_ERROR", "Power", "The power policy manager encountered a fatal error."),
    c(0xB8, "ATTEMPTED_SWITCH_FROM_DPC", "Driver", "A wait operation, attach process or yield was attempted from a deferred procedure call."),
    c(0xBE, "ATTEMPTED_WRITE_TO_READONLY_MEMORY", "Driver", "A driver attempted to write to read-only memory."),
    c(0xC1, "SPECIAL_POOL_DETECTED_MEMORY_CORRUPTION", "Driver", "Driver Verifier special pool detected memory corruption."),
    c(0xC2, "BAD_POOL_CALLER", "Driver", "The current thread made a bad pool request."),
    c(0xC4, "DRIVER_VERIFIER_DETECTED_VIOLATION", "Driver", "Driver Verifier detected a driver rule violation."),
    c(0xC5, "DRIVER_CORRUPTED_EXPOOL", "Driver", "The system accessed invalid memory at a high IRQL, typically after a driver corrupted pool."),
    c(0xCE, "DRIVER_UNLOADED_WITHOUT_CANCELLING_PENDING_OPERATIONS", "Driver", "A driver unloaded without cancelling its pending operations."),
    c(0xD1, "DRIVER_IRQL_NOT_LESS_OR_EQUAL", "Driver", "A kernel-mode driver accessed pageable memory at too high an IRQL."),
    c(0xD5, "DRIVER_PAGE_FAULT_IN_FREED_SPECIAL_POOL", "Driver", "A driver referenced memory it had already freed."),
    c(0xD8, "DRIVER_USED_EXCESSIVE_PTES", "Driver", "No more system page table entries remain."),
    c(0xE2, "MANUALLY_INITIATED_CRASH", "Manual", "The user or a debugger deliberately initiated a crash."),
    c(0xEA, "THREAD_STUCK_IN_DEVICE_DRIVER", "Driver / Graphics", "A device driver spun in an infinite loop, most often waiting for hardware to become idle."),
    c(0xED, "UNMOUNTABLE_BOOT_VOLUME", "Storage", "The boot volume could not be mounted."),
    c(0xEF, "CRITICAL_PROCESS_DIED", "System", "A critical system process terminated."),
    c(0xF4, "CRITICAL_OBJECT_TERMINATION", "System / Storage", "A process or thread crucial to system operation exited or was terminated."),
    c(0xFC, "ATTEMPTED_EXECUTE_OF_NOEXECUTE_MEMORY", "Driver", "An attempt was made to execute non-executable memory."),
    c(0x101, "CLOCK_WATCHDOG_TIMEOUT", "Hardware / Driver", "A secondary processor did not receive an expected clock interrupt in time."),
    c(0x109, "CRITICAL_STRUCTURE_CORRUPTION", "Driver / Memory", "The kernel detected corruption of critical kernel code or data."),
    c(0x10D, "WDF_VIOLATION", "Driver", "Kernel-Mode Driver Framework detected a driver error."),
    c(0x113, "VIDEO_DXGKRNL_FATAL_ERROR", "Graphics", "The DirectX graphics kernel subsystem detected a violation."),
    c(0x116, "VIDEO_TDR_FAILURE", "Graphics", "Resetting the display driver after a GPU timeout failed."),
    c(0x117, "VIDEO_TDR_TIMEOUT_DETECTED", "Graphics", "The display driver did not respond in time (normally recovered, recorded as a live dump)."),
    c(0x119, "VIDEO_SCHEDULER_INTERNAL_ERROR", "Graphics", "The video scheduler detected a fatal violation."),
    c(0x124, "WHEA_UNCORRECTABLE_ERROR", "Hardware", "A fatal hardware error was reported through the Windows Hardware Error Architecture (CPU, memory, bus or PCIe)."),
    c(0x127, "PAGE_NOT_ZERO", "Memory", "A page that should have been zero-filled was not."),
    c(0x12B, "FAULTY_HARDWARE_CORRUPTED_PAGE", "Memory / Hardware", "Single-bit errors were found in a memory page, indicating faulty hardware."),
    c(0x133, "DPC_WATCHDOG_VIOLATION", "Driver / Storage", "A deferred procedure call or the total time at DISPATCH_LEVEL or above exceeded the watchdog limit."),
    c(0x139, "KERNEL_SECURITY_CHECK_FAILURE", "Driver / Memory", "The kernel detected corruption of a critical data structure."),
    c(0x13A, "KERNEL_MODE_HEAP_CORRUPTION", "Driver / Memory", "The kernel-mode heap manager detected heap corruption."),
    c(0x144, "BUGCODE_USB3_DRIVER", "Driver / USB", "A USB 3 driver stack component detected a fatal error."),
    c(0x14F, "PDC_WATCHDOG_TIMEOUT", "Power", "A system component did not respond in time during a power transition."),
    c(0x154, "UNEXPECTED_STORE_EXCEPTION", "Storage / Memory", "The memory compression store component caught an unexpected exception."),
    c(0x157, "KERNEL_THREAD_PRIORITY_FLOOR_VIOLATION", "Driver", "A thread priority floor was violated."),
    c(0x162, "KERNEL_AUTO_BOOST_INVALID_LOCK_RELEASE", "Driver", "A lock tracked by AutoBoost was released by a thread that did not own it."),
    c(0x18B, "SECURE_KERNEL_ERROR", "System", "The secure kernel encountered a fatal error."),
    c(0x1CA, "SYNTHETIC_WATCHDOG_TIMEOUT", "Hardware / Driver", "A system-wide watchdog expired (common under virtualization)."),
    c(0x1000_007E, "SYSTEM_THREAD_EXCEPTION_NOT_HANDLED_M", "Driver", "A system thread raised an exception that no handler caught."),
    c(0x1000_008E, "KERNEL_MODE_EXCEPTION_NOT_HANDLED_M", "Driver", "A kernel-mode application raised an exception that no handler caught."),
    c(0x1000_00EA, "THREAD_STUCK_IN_DEVICE_DRIVER_M", "Driver / Graphics", "A device driver spun in an infinite loop waiting for hardware."),
    c(0xC000_021A, "STATUS_SYSTEM_PROCESS_TERMINATED", "System", "A user-mode subsystem (Winlogon or CSRSS) failed and security could no longer be guaranteed."),
    c(0xDEAD_DEAD, "MANUALLY_INITIATED_CRASH1", "Manual", "The crash was deliberately initiated."),
];

static EXCEPTIONS: &[CodeInfo] = &[
    c(0x4000_0015, "STATUS_FATAL_APP_EXIT", "Application", "The application requested a fatal exit (for example abort())."),
    c(0x8000_0003, "STATUS_BREAKPOINT", "Application", "A breakpoint instruction was hit; release software uses this to deliberately stop on an internal error."),
    c(0x8000_0004, "STATUS_SINGLE_STEP", "Application", "A single-step trap occurred."),
    c(0xC000_0005, "STATUS_ACCESS_VIOLATION", "Application / Module", "The process read, wrote or executed memory it may not access."),
    c(0xC000_0006, "STATUS_IN_PAGE_ERROR", "Storage / Network", "A page of the program or a mapped file could not be read from disk or a network share."),
    c(0xC000_0008, "STATUS_INVALID_HANDLE", "Application", "An invalid handle was used."),
    c(0xC000_0017, "STATUS_NO_MEMORY", "Memory", "Not enough virtual memory or paging file quota was available."),
    c(0xC000_001D, "STATUS_ILLEGAL_INSTRUCTION", "Application / CPU", "The CPU was asked to execute an invalid or unsupported instruction."),
    c(0xC000_0025, "STATUS_NONCONTINUABLE_EXCEPTION", "Application", "Execution continued after a non-continuable exception."),
    c(0xC000_008E, "STATUS_FLOAT_DIVIDE_BY_ZERO", "Application", "A floating-point division by zero was trapped."),
    c(0xC000_0090, "STATUS_FLOAT_INVALID_OPERATION", "Application", "An invalid floating-point operation was trapped."),
    c(0xC000_0094, "STATUS_INTEGER_DIVIDE_BY_ZERO", "Application", "An integer division by zero occurred."),
    c(0xC000_0095, "STATUS_INTEGER_OVERFLOW", "Application", "An integer overflow was trapped."),
    c(0xC000_0096, "STATUS_PRIVILEGED_INSTRUCTION", "Application", "A privileged instruction was executed in user mode."),
    c(0xC000_009A, "STATUS_INSUFFICIENT_RESOURCES", "System", "System resources were insufficient to complete the request."),
    c(0xC000_00FD, "STATUS_STACK_OVERFLOW", "Application", "A thread exhausted its stack, typically through deep or unbounded recursion."),
    c(0xC000_0135, "STATUS_DLL_NOT_FOUND", "Installation", "A required DLL could not be found."),
    c(0xC000_0139, "STATUS_ENTRYPOINT_NOT_FOUND", "Installation", "A procedure entry point was not found in a DLL (version mismatch)."),
    c(0xC000_0142, "STATUS_DLL_INIT_FAILED", "Module", "A DLL failed to initialize."),
    c(0xC000_0194, "STATUS_POSSIBLE_DEADLOCK", "Application", "A possible deadlock condition was detected."),
    c(0xC000_027B, "STATUS_STOWED_EXCEPTION", "Application", "A Windows Runtime (UWP/WinUI) app ended because of one or more unhandled errors it had stored (stowed exceptions)."),
    c(0xC000_0374, "STATUS_HEAP_CORRUPTION", "Application / Module", "The heap manager detected corruption of a heap."),
    c(0xC000_0409, "STATUS_STACK_BUFFER_OVERRUN", "Application / Module", "The process terminated itself through a fail-fast check (stack cookie, security check or explicit __fastfail)."),
    c(0xC000_041D, "STATUS_FATAL_USER_CALLBACK_EXCEPTION", "Application / Module", "An unhandled exception occurred during a kernel-to-user callback, such as a window procedure."),
    c(0xC000_0417, "STATUS_INVALID_CRUNTIME_PARAMETER", "Application", "The C runtime detected an invalid parameter."),
    c(0xC000_0420, "STATUS_ASSERTION_FAILURE", "Application", "An assertion failed."),
    c(0xC000_0602, "STATUS_FAIL_FAST_EXCEPTION", "Application", "A fail-fast exception terminated the process immediately."),
    c(0xC06D_007E, "Delay-load module not found", "Installation", "A delay-loaded DLL could not be found (MSVC delay-load helper, ERROR_MOD_NOT_FOUND)."),
    c(0xC06D_007F, "Delay-load procedure not found", "Installation", "A delay-loaded function could not be found (MSVC delay-load helper, ERROR_PROC_NOT_FOUND)."),
    c(0xCFFF_FFFF, "Application hang", "Application", "Windows ended the program after it stopped responding."),
    c(0xE043_4352, ".NET unhandled exception", "Application", "An unhandled managed (.NET CLR) exception terminated the process."),
    c(0xE06D_7363, "C++ exception", "Application", "An unhandled Microsoft C++ exception (throw) terminated the process."),
];

pub fn bugcheck(code: u32) -> Option<&'static CodeInfo> {
    BUGCHECKS.iter().find(|c| c.code == code)
}

pub fn exception(code: u32) -> Option<&'static CodeInfo> {
    EXCEPTIONS.iter().find(|c| c.code == code)
}

/// Parses `0x0000013a`, `13a`, `0xC0000005` or a decimal string (Kernel-Power 41 reports
/// `BugcheckCode` in decimal). `decimal` selects the base for strings without a `0x` prefix.
pub fn parse_code(s: &str, decimal: bool) -> Option<u64> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return u64::from_str_radix(hex, 16).ok();
    }
    if decimal {
        s.parse().ok()
    } else {
        u64::from_str_radix(s, 16).ok()
    }
}

/// Modules that typically report where an exception was raised or detected rather
/// than where the defect originated (OS loader, heap and runtime libraries).
pub fn is_generic_system_module(module: &str) -> bool {
    const GENERIC: [&str; 10] = [
        "ntdll.dll",
        "kernelbase.dll",
        "kernel32.dll",
        "ucrtbase.dll",
        "msvcrt.dll",
        "vcruntime140.dll",
        "msvcp140.dll",
        "combase.dll",
        "user32.dll",
        "coreclr.dll",
    ];
    let lower = module.to_ascii_lowercase();
    GENERIC.contains(&lower.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_codes_resolve() {
        assert_eq!(
            bugcheck(0x13A).map(|c| c.name),
            Some("KERNEL_MODE_HEAP_CORRUPTION")
        );
        assert_eq!(bugcheck(0x124).map(|c| c.area), Some("Hardware"));
        assert_eq!(
            exception(0xC000_0005).map(|c| c.name),
            Some("STATUS_ACCESS_VIOLATION")
        );
        assert!(bugcheck(0x1234_5678).is_none());
    }

    #[test]
    fn codes_are_unique() {
        for table in [BUGCHECKS, EXCEPTIONS] {
            let mut seen = std::collections::HashSet::new();
            for c in table {
                assert!(seen.insert(c.code), "duplicate {:#x}", c.code);
            }
        }
    }

    #[test]
    fn parses_code_formats() {
        assert_eq!(parse_code("0x0000013a", false), Some(0x13A));
        assert_eq!(parse_code("80000003", false), Some(0x8000_0003));
        assert_eq!(parse_code("314", true), Some(314));
        assert_eq!(parse_code("0x17", true), Some(0x17));
        assert_eq!(parse_code("zz", false), None);
        assert_eq!(parse_code("", true), None);
    }

    #[test]
    fn generic_modules_case_insensitive() {
        assert!(is_generic_system_module("KERNELBASE.dll"));
        assert!(!is_generic_system_module("libcef.dll"));
    }
}
