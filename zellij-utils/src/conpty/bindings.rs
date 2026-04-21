use anyhow::{anyhow, Result};
use std::sync::OnceLock;
use windows_sys::{
    core::HRESULT,
    Win32::{
        Foundation::{HANDLE, HMODULE},
        System::{
            Console::{COORD, HPCON},
            LibraryLoader::{GetProcAddress, LoadLibraryW},
        },
    },
};

type CreatePseudoConsoleFn = unsafe extern "system" fn(
    size: COORD,
    h_input: HANDLE,
    h_output: HANDLE,
    dw_flags: u32,
    h_pc: *mut HPCON,
) -> HRESULT;
type ResizePseudoConsoleFn = unsafe extern "system" fn(h_pc: HPCON, size: COORD) -> HRESULT;
type ClosePseudoConsoleFn = unsafe extern "system" fn(h_pc: HPCON) -> HRESULT;

pub struct ConptyApi {
    source: &'static str,
    pub create_pseudo_console: CreatePseudoConsoleFn,
    pub resize_pseudo_console: ResizePseudoConsoleFn,
    pub close_pseudo_console: ClosePseudoConsoleFn,
}

static CONPTY_API: OnceLock<Result<ConptyApi, String>> = OnceLock::new();

pub fn conpty_api() -> Result<&'static ConptyApi> {
    match CONPTY_API.get_or_init(|| unsafe { load_conpty_api().map_err(|err| err.to_string()) }) {
        Ok(api) => Ok(api),
        Err(err) => Err(anyhow!(err.clone())),
    }
}

impl ConptyApi {
    pub fn source(&self) -> &'static str {
        self.source
    }
}

unsafe fn load_conpty_api() -> Result<ConptyApi> {
    match load_conpty_api_from("conpty.dll") {
        Ok(api) => Ok(api),
        Err(conpty_err) => load_conpty_api_from("kernel32.dll").map_err(|kernel32_err| {
            anyhow!(
                "failed to load ConPTY APIs from conpty.dll ({conpty_err}) or kernel32.dll ({kernel32_err})"
            )
        }),
    }
}

unsafe fn load_conpty_api_from(library_name: &'static str) -> Result<ConptyApi> {
    let module = load_library(library_name)?;
    let create_symbol = if library_name == "conpty.dll" {
        "ConptyCreatePseudoConsole"
    } else {
        "CreatePseudoConsole"
    };
    let resize_symbol = if library_name == "conpty.dll" {
        "ConptyResizePseudoConsole"
    } else {
        "ResizePseudoConsole"
    };
    let close_symbol = if library_name == "conpty.dll" {
        "ConptyClosePseudoConsole"
    } else {
        "ClosePseudoConsole"
    };

    Ok(ConptyApi {
        source: library_name,
        create_pseudo_console: load_create_pseudo_console(module, library_name, create_symbol)?,
        resize_pseudo_console: load_resize_pseudo_console(module, library_name, resize_symbol)?,
        close_pseudo_console: load_close_pseudo_console(module, library_name, close_symbol)?,
    })
}

unsafe fn load_library(library_name: &'static str) -> Result<HMODULE> {
    let wide_library_name = wide_null(library_name);
    let module = LoadLibraryW(wide_library_name.as_ptr());
    if module.is_null() {
        return Err(anyhow!(
            "LoadLibraryW failed for {library_name}: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(module)
}

type RawConptyProc = unsafe extern "system" fn() -> isize;

union RawConptyProcCaster {
    raw: RawConptyProc,
    create_pseudo_console: CreatePseudoConsoleFn,
    resize_pseudo_console: ResizePseudoConsoleFn,
    close_pseudo_console: ClosePseudoConsoleFn,
}

unsafe fn load_symbol(
    module: HMODULE,
    library_name: &'static str,
    symbol_name: &'static str,
) -> Result<RawConptyProc> {
    let proc = GetProcAddress(module, format!("{symbol_name}\0").as_ptr());
    proc.ok_or_else(|| anyhow!("missing symbol {symbol_name} in {library_name}"))
}

unsafe fn load_create_pseudo_console(
    module: HMODULE,
    library_name: &'static str,
    symbol_name: &'static str,
) -> Result<CreatePseudoConsoleFn> {
    let raw = load_symbol(module, library_name, symbol_name)?;
    Ok(RawConptyProcCaster { raw }.create_pseudo_console)
}

unsafe fn load_resize_pseudo_console(
    module: HMODULE,
    library_name: &'static str,
    symbol_name: &'static str,
) -> Result<ResizePseudoConsoleFn> {
    let raw = load_symbol(module, library_name, symbol_name)?;
    Ok(RawConptyProcCaster { raw }.resize_pseudo_console)
}

unsafe fn load_close_pseudo_console(
    module: HMODULE,
    library_name: &'static str,
    symbol_name: &'static str,
) -> Result<ClosePseudoConsoleFn> {
    let raw = load_symbol(module, library_name, symbol_name)?;
    Ok(RawConptyProcCaster { raw }.close_pseudo_console)
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
