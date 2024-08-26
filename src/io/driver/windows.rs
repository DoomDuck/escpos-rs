// mod windows_printer;

// use std::{cell::RefCell, ffi::c_void};

// pub use self::windows_printer::WindowsPrinter;
// use crate::errors::{PrinterError, Result};
// use windows::{
//     core::{w, PWSTR},
//     Win32::{
//         Foundation::{GetLastError, HANDLE},
//         Graphics::Printing::{
//             ClosePrinter, EndDocPrinter, EndPagePrinter, OpenPrinterW, StartDocPrinterW, StartPagePrinter,
//             WritePrinter, DOC_INFO_1W,
//         }, System::Diagnostics::Debug::{FormatMessageW, FORMAT_MESSAGE_FROM_SYSTEM},
//     },
// };

// use super::Driver;

// #[derive(Debug)]
// pub struct WindowsDriver {
//     printer_name: Vec<u16>,
//     buffer: RefCell<Vec<u8>>,
// }

// impl WindowsDriver {
//     pub fn open(printer: &WindowsPrinter) -> Result<WindowsDriver> {
//         Ok(Self {
//             printer_name: printer.get_raw_vec().clone(),
//             buffer: RefCell::new(Vec::new()),
//         })
//     }

//     pub fn write_all(&self) -> Result<()> {
//         let mut error = None;
//         let mut is_printer_start = false;
//         let mut is_doc_start = false;
//         let mut is_page_start = false;
//         let mut printer_handle = HANDLE(std::ptr::null_mut());
//         #[allow(clippy::never_loop)]
//         loop {
//             unsafe {
//                 let mut document_name = w!("Raw Document").as_wide().to_vec();
//                 let mut document_type = w!("Raw").as_wide().to_vec();
//                 let mut printer_name = self.printer_name.clone();
//                 let printer_name_ptr = PWSTR(printer_name.as_mut_ptr());
//                 if OpenPrinterW(printer_name_ptr, &mut printer_handle, None).is_err() {
//                     let mut message_buffer = [0; 0x1_000];
//                     let message_len = FormatMessageW(
//                         FORMAT_MESSAGE_FROM_SYSTEM,
//                         None,
//                         GetLastError().0,
//                         0,
//                         PWSTR(message_buffer.as_mut_ptr()),
//                         message_buffer.len() as _,
//                         None,
//                     ) as usize;
//                     let message = String::from_utf16_lossy(&message_buffer[..message_len]);
//                     error = Some(PrinterError::Io(format!("Failed to open printer: {message}")));
//                     break;
//                 }
//                 is_printer_start = true;

//                 let document_info = DOC_INFO_1W {
//                     pDocName: PWSTR(document_name.as_mut_ptr()),
//                     pOutputFile: PWSTR::null(),
//                     pDatatype: PWSTR(document_type.as_mut_ptr()),
//                 };

//                 if StartDocPrinterW(printer_handle, 1, &document_info) == 0 {
//                     let mut message_buffer = [0; 0x1_000];
//                     let message_len = FormatMessageW(
//                         FORMAT_MESSAGE_FROM_SYSTEM,
//                         None,
//                         GetLastError().0,
//                         0,
//                         PWSTR(message_buffer.as_mut_ptr()),
//                         message_buffer.len() as _,
//                         None,
//                     ) as usize;
//                     let message = String::from_utf16_lossy(&message_buffer[..message_len]);
//                     error = Some(PrinterError::Io(format!("Failed to start doc: {message}")));
//                     break;
//                 }
//                 is_doc_start = true;
//                 if StartPagePrinter(printer_handle).as_bool() == false {
//                     error = Some(PrinterError::Io("Failed to start page".to_owned()));
//                     break;
//                 }
//                 is_page_start = true;

//                 let mut written: u32 = 0;
//                 let buffer = self.buffer.borrow_mut();
//                 let buffer_len = buffer.len() as u32;

//                 if !WritePrinter(
//                     printer_handle,
//                     buffer.as_ptr() as *const c_void,
//                     buffer_len,
//                     &mut written,
//                 )
//                 .as_bool()
//                 {
//                     error = Some(PrinterError::Io("Failed to write to printer".to_owned()));
//                     break;
//                 } else {
//                     if written != buffer_len {
//                         error = Some(PrinterError::Io("Failed to write all bytes to printer".to_owned()));
//                         break;
//                     }
//                 }
//             }
//             break;
//         }
//         unsafe {
//             if is_page_start {
//                 let _ = EndPagePrinter(printer_handle);
//             }
//             if is_doc_start {
//                 let _ = EndDocPrinter(printer_handle);
//             }
//             if is_printer_start {
//                 let _ = ClosePrinter(printer_handle);
//             }
//         }
//         if let Some(err) = error {
//             Err(err)
//         } else {
//             Ok(())
//         }
//     }
// }

// impl Driver for WindowsDriver {
//     fn name(&self) -> String {
//         "Windows Driver".to_owned()
//     }

//     fn write(&self, data: &[u8]) -> Result<()> {
//         let mut buffer = self.buffer.borrow_mut();
//         buffer.extend_from_slice(data);
//         Ok(())
//     }

//     fn read(&self, _buf: &mut [u8]) -> Result<usize> {
//         Ok(0)
//     }

//     fn flush(&self) -> Result<()> {
//         self.write_all()
//     }
// }

use std::{cell::{Cell, RefCell}, ffi::c_void};

pub use self::windows_printer::WindowsPrinter;
use crate::errors::{PrinterError, Result};
use windows::{
    core::{w, PWSTR},
    Win32::{
        Foundation::{BOOL, HANDLE},
        Graphics::Printing::{
            ClosePrinter, EndDocPrinter, EndPagePrinter, OpenPrinterW, StartDocPrinterW, StartPagePrinter, WritePrinter, DOCUMENTEVENT_ABORTDOC, DOC_INFO_1W
        },
    },
};

use super::Driver;

mod windows_printer;


#[derive(Debug)]
pub struct WindowsDriver {
    print_count: Cell<usize>,
    printer_name: Vec<u16>,
    buffer: RefCell<Vec<u8>>,
}

impl WindowsDriver {
    pub fn open(printer: &WindowsPrinter) -> Result<WindowsDriver> {
        Ok(Self {
            print_count: Cell::new(0),
            printer_name: printer.get_raw_vec().clone(),
            buffer: RefCell::new(Vec::new()),
        })
    }

    pub fn write_all(&self) -> Result<()> {
        let mut error: Option<PrinterError> = None;
        let mut printer_handle = HANDLE(std::ptr::null_mut());
        let mut is_printer_open = false;
        let mut is_doc_started = false;
        let mut is_page_started = false;

        unsafe {
            // Open the printer
            let mut printer_name = self.printer_name.clone();
            let printer_name_ptr = PWSTR(printer_name.as_mut_ptr());
            if OpenPrinterW(printer_name_ptr, &mut printer_handle, None).is_err() {
                error = Some(PrinterError::Io("Failed to open printer".to_owned()));
                eprintln!("Error: {:?}", error);
            } else {
                is_printer_open = true;
                let document_name = format!("Raw document #{}", self.print_count.get());
                self.print_count.set(self.print_count.get() + 1);
                let document_name_wide: Vec<_> = document_name.encode_utf16().chain([0]).collect();
                // Start the document
                let document_info = DOC_INFO_1W {
                    pDocName: PWSTR(document_name_wide.as_ptr() as *mut _),
                    pOutputFile: PWSTR::null(),
                    pDatatype: PWSTR(w!("Raw").as_wide().as_ptr() as *mut _),
                };

                if StartDocPrinterW(printer_handle, 1, &document_info) == 0 {
                    error = Some(PrinterError::Io("Failed to start doc".to_owned()));
                    eprintln!("Error: {:?}", error);
                } else {
                    is_doc_started = true;
                    // // Start the page
                    // if !StartPagePrinter(printer_handle).as_bool() {
                    //     error = Some(PrinterError::Io("Failed to start page".to_owned()));
                    //     eprintln!("Error: {:?}", error);
                    // } else {
                    //     is_page_started = true;
                        // Write to the printer
                        let buffer = self.buffer.borrow();
                        let mut written: u32 = 0;
                        if !WritePrinter(
                            printer_handle,
                            buffer.as_ptr() as *const c_void,
                            buffer.len() as u32,
                            &mut written,
                        )
                        .as_bool()
                        {
                            error = Some(PrinterError::Io("Failed to write to printer".to_owned()));
                            eprintln!("Error: {:?}", error);
                        } else if written != buffer.len() as u32 {
                            error = Some(PrinterError::Io("Failed to write all bytes to printer".to_owned()));
                            eprintln!("Error: {:?}", error);
                        }
                    // }
                }
            }
        }

        // Clean up resources
        unsafe {
            // if is_page_started {
            //     if EndPagePrinter(printer_handle) == BOOL(0) {
            //         eprintln!("Warning: Failed to end page");
            //     }
            // }
            if is_doc_started {
                if EndDocPrinter(printer_handle) == BOOL(0) {
                    eprintln!("Warning: Failed to end document");
                }
            }
            if is_printer_open {
                if let Err(e) = ClosePrinter(printer_handle) {
                    eprintln!("Warning: Failed to close printer: {:?}", e);
                }
            }
        }

        // Return result
        if let Some(err) = error {
            Err(err)
        } else {
            Ok(())
        }
    }
}

impl Driver for WindowsDriver {
    fn name(&self) -> String {
        "Windows Driver".to_owned()
    }

    fn write(&self, data: &[u8]) -> Result<()> {
        let mut buffer = self.buffer.borrow_mut();
        buffer.extend_from_slice(data);
        Ok(())
    }

    fn read(&self, _buf: &mut [u8]) -> Result<usize> {
        Ok(0)
    }

    fn flush(&self) -> Result<()> {
        self.write_all()
    }
}