#![allow(dead_code)]

#[allow(non_snake_case)]
#[repr(C, align(4))]
#[derive(Default, Debug, PartialEq)]
pub struct CRunningScript {
    m_pNext: u32,
    m_pPrev: u32,
    pub m_abScriptName: [u8; 8],
    pub m_nBaseIp: usize,
    pub m_nIp: usize,
    pub m_anStack: [usize; 8],
    pub m_nStackPointer: u16,
    pub m_anLocalVariables: [i32; 32],
    pub m_nTimerA: i32,
    pub m_nTimerB: i32,
    m_bIsActive: bool,
    pub m_bCondResult: bool,
    pub m_bIsMissionScript: bool,
    m_bIsExternal: bool,
    m_bOverridesTextBox: bool,
    pub m_nExternalType: i8,
    pub m_nWakeTime: u32,
    m_nAndOrState: u16,
    pub m_bNotFlag: bool,
    m_bDeatharrestEnabled: bool,
    m_bDeatharrestExecuted: bool,
    m_nSkipScenePos: i32,
    pub m_bMissionFlag: bool,
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
    pub fn CLEO_GetScriptFilename(thread: *const CRunningScript) -> *const i8;
    pub fn CLEO_GetScriptWorkDir(thread: *const CRunningScript) -> *const i8;

    pub fn CLEO_GetScriptVersion(thread: *const CRunningScript) -> u32;
    // Writes `CCustomScript::m_compatVer`, i.e. the mode the opcode handlers check.
    pub fn CLEO_SetScriptVersion(thread: *mut CRunningScript, version: u32);
    pub fn CLEO_GetScriptDebugMode(thread: *const CRunningScript) -> bool;
    pub fn CLEO_SetScriptDebugMode(thread: *mut CRunningScript, enabled: bool);
    // Size of the script in bytes, 0 if unknown.
    pub fn CLEO_GetScriptSize(thread: *const CRunningScript) -> usize;
    pub fn CLEO_GetScriptInfoStr(
        thread: *const CRunningScript,
        currLineInfo: bool,
        buf: *mut i8,
        bufSize: u32,
    );

    pub fn CLEO_ResolvePath(thread: *mut CRunningScript, inOutPath: *mut i8, pathMaxLen: u32);
    pub fn CLEO_TerminateScript(thread: *mut CRunningScript);
    pub fn CLEO_CreateCustomScript(
        fromThread: *mut CRunningScript,
        filePath: *const i8,
        label: i32,
    ) -> *mut CRunningScript;

    // Full absolute path to the script file.
    pub fn CLEO_GetScriptFileFullPath(thread: *const CRunningScript, buf: *mut i8, bufSize: u32);
}
