#![allow(dead_code)]

use std::ffi::c_void;

/// GTA San Andreas 1.0 US `CRunningScript`, from `cleo_sdk/CLEO.h` (which is `#pragma pack(1)`, so
/// the packing here has to reproduce it field for field).
#[allow(non_snake_case)]
#[repr(C, align(4))]
#[derive(Default, Debug, PartialEq)]
pub struct CRunningScript {
    /// 0x00: next script in the queue.
    pub m_pNext: u32,
    /// 0x04: previous script in the queue.
    pub m_pPrev: u32,
    /// 0x08: name given by `03A4`, not the file name.
    pub m_abScriptName: [u8; 8],
    pub m_nBaseIp: usize,
    pub m_nIp: usize,
    pub m_anStack: [usize; 8],
    pub m_nStackPointer: u16,
    pub m_anLocalVariables: [i32; 32],
    pub m_nTimerA: i32,
    pub m_nTimerB: i32,
    pub m_bIsActive: bool,
    pub m_bCondResult: bool,
    /// 0xC6: `bUseMissionCleanup` - the mission blob is wiped when the script ends.
    pub m_bUseMissionCleanup: bool,
    /// 0xC7: `bIsExternal` - the thread comes from `script.img`.
    pub m_bIsExternal: bool,
    /// 0xC8: `bTextBlockOverride`.
    pub m_bTextBlockOverride: bool,
    /// 0xC9: `bExternalType`; CLEO starts it at -1.
    pub m_nExternalType: i8,
    pub m_nWakeTime: u32,
    /// `LogicalOp`: counts the `and`/`or` keywords still to come in a condition list.
    pub m_nAndOrState: u16,
    pub m_bNotFlag: bool,
    /// 0xD3: `bWastedBustedCheck` - the death-arrest check is armed.
    pub m_bWastedBustedCheck: bool,
    /// 0xD4: `bWastedOrBusted` - the check fired.
    pub m_bWastedOrBusted: bool,
    /// 0xD8: `SceneSkipIP`.
    pub m_nSceneSkipIp: i32,
    /// 0xDC: `bIsMission`.
    pub m_bIsMission: bool,
    /// 0xDD: CLEO's `ScmFunction`, a `WORD` at an odd offset: kept as bytes because a
    /// `repr(C)` `u16` would be aligned to 0xDE.
    pub m_abScmFunction: [u8; 2],
    /// 0xDF: the bitfield `bIsCustom : 1, bIsProcessing : 1`.
    pub m_bScriptFlags: u8,
}

impl CRunningScript {
    /// The script name from `03A4`, cut at its terminator.
    pub fn name(&self) -> String {
        let used = self
            .m_abScriptName
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(self.m_abScriptName.len());
        String::from_utf8_lossy(&self.m_abScriptName[..used]).into_owned()
    }

    /// CLEO's `ScmFunction` (`0x0AB1` bookkeeping).
    pub fn scm_function(&self) -> u16 {
        u16::from_le_bytes(self.m_abScmFunction)
    }

    /// `bIsCustom`: the thread is a CLEO script rather than one from `main.scm`.
    pub fn is_custom(&self) -> bool {
        self.m_bScriptFlags & 0x01 != 0
    }

    /// `bIsProcessing`: an opcode handler started and has not finished. Not shown: while
    /// the panel can read it, no handler is running, so it is always false.
    #[allow(dead_code)]
    pub fn is_processing(&self) -> bool {
        self.m_bScriptFlags & 0x02 != 0
    }
}

#[cfg(target_pointer_width = "32")]
assert_eq_size!(CRunningScript, [u8; 0xE0]);

pub const CLEO_VER_3: u32 = 0x03000000;
pub const CLEO_VER_4_MIN: u32 = 0x04000000;
pub const CLEO_VER_4_2: u32 = 0x04020000;
pub const CLEO_VER_4_3: u32 = 0x04030000;
pub const CLEO_VER_4_4: u32 = 0x04040000;
pub const CLEO_VER_4: u32 = CLEO_VER_4_4;
pub const CLEO_VER_5: u32 = 0x05000000;
pub const CLEO_VER_CUR: u32 = 0x05010000;

/// `CLEO::eCallbackId::ScriptOpcodeProcessBefore`.
pub const CB_OPCODE_BEFORE: u32 = 8;
/// `CLEO::eCallbackId::ScriptOpcodeProcessAfter`.
pub const CB_OPCODE_AFTER: u32 = 9;
/// `CLEO::eCallbackId::Log`.
pub const CB_LOG: u32 = 12;

/// `CLEO::OpcodeResult`, which is `enum : char`.
pub const OR_NONE: i8 = -2;
pub const OR_ERROR: i8 = -1;
pub const OR_CONTINUE: i8 = 0;
pub const OR_INTERRUPT: i8 = 1;

/// The value CLEO parks a script with: `CLEO_Utils.h`'s `Suspended_Wake_Time`.
pub const SUSPENDED_WAKE_TIME: u32 = 0xFFFFFFFF;

/// `CTimer::m_snTimeInMilliseconds`, the clock `wait` compares its wake time against.
/// San Andreas 1.0 US only, like the rest of this overlay.
pub const TIMER_MS: usize = 0xB7CB84;

/// `CTheScripts::pActiveScripts`: the head of the engine's thread list. CLEO adds its own
/// scripts to the same list, so one walk yields the game's threads and CLEO's together.
pub const ACTIVE_SCRIPTS: usize = 0xA8B42C;

/// `sizeof CTheScripts::ScriptSpace`, the block `main.scm` bytecode is loaded into.
pub const SCM_BLOCK_SIZE: usize = 0x30D40;

/// The mission block that follows it, where `script.img` bytecode is loaded. This is the
/// retail size; the block is really as big as the largest mission, which the SCM header
/// records - see [`MISSION_BLOCK_REF`]'s sibling constants.
pub const MISSION_BLOCK_SIZE: usize = 0x10D88;

/// The mission block's base, read from the operand inside the resource loader rather than
/// assumed, so a limit adjuster that moved the block is followed. This is exactly what
/// CLEO's `MA_MISSION_BLOCK_REF` does with it.
pub const MISSION_BLOCK_REF: usize = 0x004899D7;

/// `CTheScripts::LargestMissionScriptSize`, from the SCM header: the biggest mission in the
/// script file, which is also what the mission block is sized to hold.
pub const LARGEST_MISSION_SIZE: usize = 0xA444C0;

/// `CTheScripts::missionCount`: how many entries the header's mission offset table has.
pub const MISSION_COUNT: usize = 0xA444BC;

/// `CTheScripts::missionOffsets`, the per-mission offsets out of the SCM header. Entry `i` is
/// where mission `i` begins in the script stream - the main script takes the first
/// `MainScriptSize` bytes, and the missions follow it - so two neighbours in the table are a
/// mission's extent, and the table's last entry plus the largest mission is the end of the
/// file. A slot with no mission has the next slot's offset.
pub const MISSION_OFFSETS: usize = 0xA444C8;

/// `OpcodeResult WINAPI OnScriptOpcodeProcessBefore(CRunningScript*, DWORD opcode)`.
pub type OpcodeHookBefore = unsafe extern "system" fn(*mut CRunningScript, u32) -> i8;
/// `OpcodeResult WINAPI OnScriptOpcodeProcessAfter(CRunningScript*, DWORD, OpcodeResult)`.
pub type OpcodeHookAfter = unsafe extern "system" fn(*mut CRunningScript, u32, i8) -> i8;
/// `void WINAPI OnLog(eLogLevel level, const char* msg)`.
pub type LogHook = unsafe extern "system" fn(u32, *const i8);

#[cfg_attr(target_arch = "x86", link(name = "CLEO"))]
extern "stdcall" {
    pub fn CLEO_GetVersion() -> u32;
    pub fn CLEO_GetVersionStr() -> *const i8;

    // CLEO directories (null terminated absolute paths)
    pub fn CLEO_GetGameDirectory() -> *const i8;
    pub fn CLEO_GetCleoDirectory() -> *const i8;
    pub fn CLEO_GetLogDirectory() -> *const i8;

    // .cleo_config.ini management ([Plugins] section)
    pub fn CLEO_GetConfigInt(key: *const i8, defaultValue: i32) -> i32;
    pub fn CLEO_GetConfigFloat(key: *const i8, defaultValue: f32) -> f32;
    pub fn CLEO_GetConfigText(
        key: *const i8,
        defaultValue: *const i8,
        buffer: *mut i8,
        bufferSize: u32,
    ) -> u32;
    pub fn CLEO_SetConfigInt(key: *const i8, value: i32) -> i32;
    pub fn CLEO_SetConfigFloat(key: *const i8, value: f32) -> i32;
    pub fn CLEO_SetConfigText(key: *const i8, value: *const i8) -> i32;

    pub fn CLEO_GetNumberOfCustomScripts() -> usize;
    pub fn CLEO_GetCustomScriptByIndex(index: usize) -> *mut CRunningScript;
    pub fn CLEO_GetScriptFilename(script: *const CRunningScript) -> *const i8;
    pub fn CLEO_GetScriptWorkDir(script: *const CRunningScript) -> *const i8;

    pub fn CLEO_GetScriptVersion(script: *const CRunningScript) -> u32;
    // Writes `CCustomScript::m_compatVer`, i.e. the mode the opcode handlers check.
    pub fn CLEO_SetScriptVersion(script: *mut CRunningScript, version: u32);
    pub fn CLEO_GetScriptDebugMode(script: *const CRunningScript) -> bool;
    pub fn CLEO_SetScriptDebugMode(script: *mut CRunningScript, enabled: bool);
    // Size of the script in bytes, 0 if unknown.
    pub fn CLEO_GetScriptSize(script: *const CRunningScript) -> usize;
    pub fn CLEO_GetScriptInfoStr(
        script: *const CRunningScript,
        currLineInfo: bool,
        buf: *mut i8,
        bufSize: u32,
    );

    pub fn CLEO_ResolvePath(script: *mut CRunningScript, inOutPath: *mut i8, pathMaxLen: u32);
    pub fn CLEO_TerminateScript(script: *mut CRunningScript);
    pub fn CLEO_CreateCustomScript(
        fromThread: *mut CRunningScript,
        filePath: *const i8,
        label: i32,
    ) -> *mut CRunningScript;

    // Full absolute path to the script file.
    pub fn CLEO_GetScriptFileFullPath(script: *const CRunningScript, buf: *mut i8, bufSize: u32);

    // Per-opcode hooks. A `Before` callback that returns anything but `OR_NONE` is taken
    // to have handled the opcode; an `After` callback that returns anything but `OR_NONE`
    // overwrites the handler's result, and `OR_INTERRUPT` (the largest value) wins.
    pub fn CLEO_RegisterCallback(id: u32, func: *mut c_void);
    pub fn CLEO_UnregisterCallback(id: u32, func: *mut c_void);

    // Offset of a code position within the script, counted from its base.
    pub fn CLEO_GetScriptBaseRelativeOffset(script: *const CRunningScript, codePos: *const u8) -> u32;
    // Length of the current `cleo_call` chain.
    pub fn CLEO_GetCleoCallStackSize(script: *mut CRunningScript) -> u32;
    // Whether CLEO still owns this pointer. Scripts may be inactive or pending deletion.
    pub fn CLEO_IsValidScriptPtr(script: *const CRunningScript) -> i32;
    // Base of the main script block, i.e. the array of global variables.
    pub fn CLEO_GetScmMainData() -> *mut u8;
}
