# Windows Console and Pane Control Architecture

This document describes the target Windows console architecture for Zellij.
It focuses on the final module boundaries and responsibilities rather than the
migration steps needed to get there.

## TL;DR

- Windows console function calls should no longer be scattered across
  `zellij-client` and `zellij-server`.
- Shared Windows-specific behavior should live in `zellij-utils`.
- True ConPTY lifecycle APIs should prefer `conpty.dll` and fall back to
  `kernel32.dll`.
- Traditional console APIs should keep their classic semantics and remain backed
  by `kernel32.dll`.
- Pane control should be modeled as high-level intent (`interrupt`, `terminate`,
  `write_input`) rather than raw Win32 calls like `GenerateConsoleCtrlEvent`.

## Problem Statement

Zellij currently mixes several Windows models in multiple places:

- classic console APIs
- ConPTY lifecycle APIs
- VT sequence-based pane input
- process control and termination

These are currently spread across:

- `zellij-client/src/stdin_handler_windows.rs`
- `zellij-client/src/os_input_output_windows.rs`
- `zellij-server/src/os_input_output_windows.rs`
- `zellij-server/src/lib.rs`

This creates a few long-term problems:

- no single ownership point for Windows console behavior
- direct dependence on static `windows-sys` console imports in multiple crates
- control-flow decisions are expressed as raw API choices instead of user intent
- ConPTY-hosted environments and classic console environments are not clearly
  separated at the architectural level

The target architecture centralizes this behavior in shared modules under
`zellij-utils`.

## Design Goals

The target architecture should:

- centralize Windows console behavior in one shared place
- preserve existing `windows-sys` types and constants where possible
- isolate DLL loading policy from business logic
- separate ConPTY lifecycle from classic console operations
- model pane control as intent-driven behavior rather than raw signal APIs
- allow Windows-specific behavior to vary internally without changing the rest
  of the codebase

## Non-Goals

This architecture does not aim to:

- replace all Windows APIs with ConPTY-native equivalents
- treat every `Win32::System::Console` function as if it belongs in
  `conpty.dll`
- expose raw DLL loading details to callers
- force client and server to know about backend-specific control semantics

## Architectural Overview

The target architecture introduces two shared Windows modules in
`zellij-utils`:

- `zellij-utils/src/windows_console.rs`
- `zellij-utils/src/windows_pane_control.rs`

### `windows_console.rs`

This module owns low-level Windows console and ConPTY function access.

Its responsibilities are:

- runtime resolution of selected Win32 function symbols
- DLL preference policy
- wrapper functions around dynamically loaded console and ConPTY calls
- shared access to low-level Windows console operations used by client/server

It does **not** decide pane-control policy.

### `windows_pane_control.rs`

This module owns high-level Windows pane control behavior.

Its responsibilities are:

- expressing pane control in terms of user intent
- selecting the appropriate backend strategy for a pane/session
- routing soft interrupts, input writes and termination through the correct
  Windows mechanism
- hiding classic console vs ConPTY differences from higher-level code

It does **not** own DLL loading.

## Shared Types and Constants

The architecture keeps compile-time Windows SDK types and constants from
`windows-sys`.

This includes values such as:

- `HPCON`
- `COORD`
- `STD_INPUT_HANDLE`
- `CTRL_C_EVENT`
- `CTRL_BREAK_EVENT`
- `CTRL_CLOSE_EVENT`
- `ENABLE_*` console mode flags

These are compile-time definitions and do not need to be dynamically loaded.

## Module: `zellij-utils/src/windows_console.rs`

### Responsibilities

This module provides the shared low-level Windows console access layer.

It should:

- resolve function pointers once and reuse them for the process lifetime
- prefer `conpty.dll` for true ConPTY lifecycle APIs
- fall back to `kernel32.dll` for ConPTY lifecycle APIs when needed
- use `kernel32.dll` for traditional console APIs
- expose a stable wrapper surface to the rest of Zellij

### DLL Resolution Policy

The target policy is:

#### ConPTY lifecycle APIs

These functions should try `conpty.dll` first and then `kernel32.dll`:

- `CreatePseudoConsole`
- `ClosePseudoConsole`
- `ResizePseudoConsole`

#### Traditional console APIs

These functions should remain kernel32-backed:

- `GetConsoleMode`
- `SetConsoleMode`
- `GetStdHandle`
- `SetConsoleCtrlHandler`
- `GenerateConsoleCtrlEvent`

Even though some of these operations still matter in ConPTY-hosted scenarios,
they are not modeled as ConPTY lifecycle exports.

### Public API Shape

The module should expose wrappers rather than raw `libloading` handles.

For example:

```rust
#[cfg(windows)]
pub fn create_pseudo_console(/* ... */) -> anyhow::Result<HPCON>;

#[cfg(windows)]
pub fn close_pseudo_console(hpcon: HPCON);

#[cfg(windows)]
pub fn resize_pseudo_console(hpcon: HPCON, size: COORD) -> anyhow::Result<()>;

#[cfg(windows)]
pub fn get_console_mode(handle: HANDLE) -> anyhow::Result<u32>;

#[cfg(windows)]
pub fn set_console_mode(handle: HANDLE, mode: u32) -> anyhow::Result<()>;

#[cfg(windows)]
pub fn get_std_handle(which: u32) -> anyhow::Result<HANDLE>;

#[cfg(windows)]
pub fn set_console_ctrl_handler(
    handler: Option<unsafe extern "system" fn(u32) -> BOOL>,
    add: bool,
) -> anyhow::Result<()>;

#[cfg(windows)]
pub fn generate_console_ctrl_event(event: u32, process_group_id: u32) -> anyhow::Result<()>;
```

The exact signatures may differ, but the architectural intent is that all raw
Windows console function invocation goes through this module.

### Initialization Model

Function resolution should happen lazily and only once.

Acceptable approaches include:

- `OnceLock`
- `LazyLock`

The module should cache resolved symbols and avoid repeated DLL lookups.

### Error Handling

Resolution failures and call failures should be surfaced as normal Zellij
errors with context.

This module should prefer returning `Result` values rather than panicking.

## Module: `zellij-utils/src/windows_pane_control.rs`

### Responsibilities

This module provides a higher-level Windows pane control adapter.

It owns the policy for operations such as:

- soft interruption
- input injection
- graceful termination
- forceful termination

It should not expose raw Win32 signal semantics as the primary public API.

### Why This Module Exists

The existing `send_sigint(pid)` model is too low-level and too Unix-shaped for
Windows.

The real user intent is not “call `GenerateConsoleCtrlEvent`”.
The real user intent is closer to:

- interrupt the running pane program
- send input to the pane
- terminate the pane process
- forcibly kill the pane process if graceful methods fail

This module encodes those higher-level intents explicitly.

### Public API Shape

The public surface should be façade-oriented.

For example:

```rust
pub struct WindowsPaneControlAdapter {
    // backend selection and shared policy
}

impl WindowsPaneControlAdapter {
    pub fn interrupt_pane(&self, target: &PaneControlTarget) -> anyhow::Result<()>;
    pub fn write_input(&self, target: &PaneControlTarget, bytes: &[u8]) -> anyhow::Result<usize>;
    pub fn terminate_pane(&self, target: &PaneControlTarget) -> anyhow::Result<()>;
    pub fn force_terminate_pane(&self, target: &PaneControlTarget) -> anyhow::Result<()>;
}
```

The exact target type may differ, but the adapter should take a logical pane
control target rather than exposing separate raw per-API parameter lists.

### Backend Strategies

The adapter should hide multiple Windows backend strategies behind one façade.

#### ConPTY control backend

Used for panes backed by ConPTY handles and pipes.

Preferred behaviors:

- `interrupt_pane`: use ConPTY-native input injection (eg. ETX / `\x03`) via the
  pane input pipe rather than making `GenerateConsoleCtrlEvent` the primary
  mechanism
- `write_input`: write directly to the pane input handle
- `terminate_pane`: use process termination policy appropriate to the pane
- `force_terminate_pane`: use forceful process kill

#### Classic console control backend

Used when a traditional console-backed control path is actually appropriate.

Possible behaviors:

- `interrupt_pane`: use classic control-event behavior where valid
- `write_input`: use the appropriate classic console input path
- termination paths as needed

#### Process fallback backend

Used when the environment cannot support a graceful control operation.

Possible behaviors:

- if interruption is not supported, fail with context or fall back according to
  adapter policy
- termination and forceful kill remain available through process APIs

### Backend Selection

Backend selection should be an internal concern.

Higher-level Zellij code should not need to know:

- which DLL a function came from
- whether a pane is interrupted by control event or PTY input injection
- whether the implementation is classic-console or ConPTY-native

The adapter should choose based on pane/session state available from the Windows
backend.

## Expected Integration Points

The target architecture affects the following code areas.

### Server-side Windows backend

- `zellij-server/src/os_input_output_windows.rs`

This file should stop being the place where control semantics are decided.
It should use `windows_console.rs` for low-level APIs and
`windows_pane_control.rs` for pane-control intent.

### Server-side OS abstraction

- `zellij-server/src/os_input_output.rs`

This layer should depend on higher-level pane-control methods rather than raw
Win32 signal-oriented methods.

### PTY management

- `zellij-server/src/pty.rs`

Operations such as “send interrupt to pane” should be expressed in terms of the
new adapter rather than directly binding semantic meaning to
`GenerateConsoleCtrlEvent`.

### Client-side Windows console handling

- `zellij-client/src/stdin_handler_windows.rs`
- `zellij-client/src/os_input_output_windows.rs`
- `zellij-server/src/lib.rs`

These areas should use `windows_console.rs` for shared low-level Windows console
operations so client and server do not maintain separate raw import patterns.

## Ownership Rules

The target architecture follows these ownership rules:

### `windows_console.rs` owns

- dynamic Windows console/ConPTY function resolution
- DLL preference rules
- low-level wrapper calls for selected Windows console functions

### `windows_pane_control.rs` owns

- pane control policy
- backend selection for Windows pane control
- interrupt/input/termination semantics

### Client/server crates own

- their existing business logic
- passing the right pane/session context into the shared Windows modules
- reacting to returned errors

They should not own raw Windows console function policy anymore.

## Control Semantics

The target architecture intentionally distinguishes between:

- a pane interruption request
- a classic Windows console control event
- PTY input injection
- process termination

These are related but not identical operations.

The architecture therefore standardizes on **intent-first semantics**:

- `interrupt_pane` means “attempt a soft interrupt appropriate for this pane”
- `terminate_pane` means “attempt a normal termination path appropriate for this
  pane”
- `force_terminate_pane` means “kill the underlying process when gentler methods
  are not sufficient”

This lets Windows implementations vary internally while preserving stable
meaning for the rest of the codebase.

## Resulting Benefits

If this architecture is followed, Zellij gains:

- one shared ownership point for Windows console access
- one shared ownership point for Windows pane-control semantics
- clearer separation between classic console behavior and ConPTY behavior
- less scattered Windows-specific policy in client/server crates
- easier future support for host-specific Windows behavior without spreading it
  through the codebase

## Files in the Target Architecture

The target architecture introduces or reshapes the following files:

- `docs/WINDOWS_CONSOLE_ARCHITECTURE.md`
- `zellij-utils/src/windows_console.rs`
- `zellij-utils/src/windows_pane_control.rs`
- `zellij-server/src/os_input_output_windows.rs`
- `zellij-server/src/os_input_output.rs`
- `zellij-server/src/pty.rs`
- `zellij-client/src/stdin_handler_windows.rs`
- `zellij-client/src/os_input_output_windows.rs`
- `zellij-server/src/lib.rs`
