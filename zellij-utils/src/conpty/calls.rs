#![allow(non_snake_case)]

use anyhow::{anyhow, Result};
use std::mem::MaybeUninit;
use windows_sys::Win32::{
    Foundation::HANDLE,
    System::Console::{COORD, HPCON},
};

use super::bindings::conpty_api;

pub unsafe fn CreatePseudoConsole(
    size: COORD,
    hInput: HANDLE,
    hOutput: HANDLE,
    dwFlags: u32,
) -> Result<HPCON> {
    let api = conpty_api()?;
    let mut console_handle_uninit = MaybeUninit::<HPCON>::uninit();
    let result_code = (api.create_pseudo_console)(
        size,
        hInput,
        hOutput,
        dwFlags,
        console_handle_uninit.as_mut_ptr(),
    );

    check_hresult(result_code, format!("{}!CreatePseudoConsole", api.source()))?;

    Ok(console_handle_uninit.assume_init())
}

pub unsafe fn ResizePseudoConsole(hPC: HPCON, size: COORD) -> Result<()> {
    let api = conpty_api()?;
    let result_code = (api.resize_pseudo_console)(hPC, size);

    check_hresult(result_code, format!("{}!ResizePseudoConsole", api.source()))
}

pub unsafe fn ClosePseudoConsole(hPC: HPCON) -> Result<()> {
    let api = conpty_api()?;
    let result_code = (api.close_pseudo_console)(hPC);

    check_hresult(result_code, format!("{}!ClosePseudoConsole", api.source()))
}

fn check_hresult(result_code: i32, operation: String) -> Result<()> {
    if result_code < 0 {
        return Err(anyhow!(
            "{operation} failed with HRESULT 0x{:08X}",
            result_code as u32
        ));
    }
    Ok(())
}
