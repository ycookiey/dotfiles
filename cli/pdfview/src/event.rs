use crate::cache::Quality;
use crate::error::PdfViewError;
use crate::render::RenderedPage;
use crate::terminal::TerminalDims;

#[derive(Debug)]
pub enum Event {
    Input(Input),
    Resize { cols: u16, rows: u16 },
    DebouncedResize(TerminalDims),
    RenderComplete {
        page: u32,
        quality: Quality,
        result: Result<RenderedPage, PdfViewError>,
    },
    Tick,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    Next,
    Prev,
    First,
    Last,
    Goto(u32),
    GotoStart,
    Digit(char),
    Quit,
    Noop,
}
