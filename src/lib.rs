//! # CSharp_Binder
//!
//! CSharp_Binder is a tool written to generate C# bindings for a Rust FFI (Foreign Function Interface).
//! By interacting over extern C functions, this allows you to easily call Rust functions from C#,
//! without having to write the extern C# functions yourself.
//!
//! CSharp_Binder will when given a Rust script, parse this script, and extract any functions marked as
//! extern "C", enums with a ``[repr(u*)]`` attribute, and structs with a ``#[repr(C)]`` attribute. It
//! will then convert these into appropriate representations in C#.
//!
//! CSharp_Binder will also extract Rust documentation on functions, enums and their variants, and
//! on structs and their fields, and convert it into XML Documentation on the generated C# code.
//!
//! Note that CSharp_Binder uses syn to parse Rust scripts, so macros will not be expanded! If you
//! have functions, structs, or enums that need to be extracted inside macros, make sure to run them
//! to something like cargo-expand first.
//!
//! # Examples
//!
//! Example:
//! ```
//! use csharp_binder::{CSharpConfiguration, CSharpBuilder};
//!
//! fn main(){
//!     let mut configuration = CSharpConfiguration::new();
//!     let rust_file = r#"
//!     /// Just a random return enum
//!     #[repr(u8)]
//!     enum ReturnEnum {
//!         Val1,
//!         Val2,
//!     }
//!     
//!     /// An input struct we expect
//!     #[repr(C)]
//!     struct InputStruct {
//!         field_a: u16,
//!         /// This field is used for floats!
//!         field_b: f64,
//!     }
//!     
//!     pub extern "C" fn foo(a: InputStruct) -> ReturnEnum {
//!     }
//!     "#;
//!     let mut builder = CSharpBuilder::new(rust_file, "foo", &mut configuration)
//!                         .expect("Failed to parse file");
//!     builder.set_namespace("MainNamespace");
//!     builder.set_type("InsideClass");
//!     let script = builder.build().expect("Failed to build");
//! }
//!```
//!
//! This would return the following C# code:
//!
//! ```cs
//! // Automatically generated, do not edit!
//! using System;
//! using System.Runtime.InteropServices;
//!
//! namespace MainNamespace
//! {
//!    internal static class InsideClass
//!    {
//!        /// <summary>
//!        /// Just a random return enum
//!        /// </summary>
//!         public enum ReturnEnum : byte
//!         {
//!             Val1,
//!             Val2,
//!         }
//!
//!         /// <summary>
//!         /// An input struct we expect
//!         /// </summary>
//!         [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
//!         public struct InputStruct
//!         {
//!             /// <remarks>u16</remarks>
//!             public readonly ushort FieldA;
//!             /// <summary>
//!             /// This field is used for floats!
//!             /// </summary>
//!             /// <remarks>f64</remarks>
//!             public readonly double FieldB;
//!         }
//!
//!         /// <param name="a">InputStruct</param>
//!         /// <returns>ReturnEnum</returns>
//!         [DllImport("foo", CallingConvention = CallingConvention.Cdecl, EntryPoint="foo")]
//!         internal static extern ReturnEnum Foo(InputStruct a);
//!
//!     }
//! }
//! ```
//!
use crate::builder::{build_csharp, parse_script};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Formatter;

mod builder;

#[cfg(test)]
mod tests;

pub(crate) struct CSharpType {
    pub namespace: Option<String>,
    pub inside_type: Option<String>,
    pub real_type_name: String,
}

/// This struct holds the generic data used between multiple builds. Currently this only holds the
/// type registry, but further features such as ignore patterns will likely be added here.
pub struct CSharpConfiguration {
    known_types: HashMap<String, CSharpType>,
}

impl Default for CSharpConfiguration {
    fn default() -> Self {
        Self::new()
    }
}

impl CSharpConfiguration {
    pub fn new() -> Self {
        Self {
            known_types: HashMap::new(),
        }
    }

    /// Register a type the converter should know about.
    ///
    /// Useful if you use a type on the Rust side that you know has a C# representation without first
    /// passing it through the C#builder. This function takes the Rust type name, along with an optional
    /// C# namespace, optional containing type, and the actual C# type name.
    pub fn add_known_type(
        &mut self,
        rust_type_name: &str,
        csharp_namespace: Option<String>,
        csharp_inside_type: Option<String>,
        csharp_type_name: String,
    ) {
        self.known_types.insert(
            rust_type_name.to_string(),
            CSharpType {
                namespace: csharp_namespace,
                inside_type: csharp_inside_type,
                real_type_name: csharp_type_name,
            },
        );
    }

    pub(crate) fn get_known_type(&self, rust_type_name: &str) -> Option<&CSharpType> {
        self.known_types.get(rust_type_name)
    }
}

/// The CSharpBuilder is used to load a Rust script string, and convert it into the appropriate C#
/// script as a string.
pub struct CSharpBuilder<'a> {
    configuration: RefCell<&'a mut CSharpConfiguration>,
    dll_name: String,
    usings: Vec<String>,
    tokens: syn::File,
    namespace: Option<String>,
    type_name: Option<String>,
}

impl<'a> CSharpBuilder<'a> {
    /// Creates a new C# Builder from a Rust script string, the name of the library C# is going to
    /// make calls to (the .so/.dll file), and a configuration.
    ///
    /// Note that this will immediately parse the rust script and extract its symbols. As such, this
    /// can return a parse error.
    pub fn new(
        script: &str,
        dll_name: &str,
        configuration: &'a mut CSharpConfiguration,
    ) -> Result<CSharpBuilder<'a>, Error> {
        match parse_script(script) {
            Ok(tokens) => Ok(CSharpBuilder {
                configuration: RefCell::new(configuration),
                dll_name: dll_name.to_string(),
                // Load the default usings.
                usings: vec![
                    "System".to_string(),
                    "System.Runtime.InteropServices".to_string(),
                ],
                tokens,
                namespace: None,
                type_name: None,
            }),
            Err(e) => Err(Error::from(e)),
        }
    }

    /// This function will return the C# script. Should be called after the C# Builder is setup.
    pub fn build(&mut self) -> Result<String, Error> {
        build_csharp(self)
    }

    /// Sets the namespace the C# script should use to generate its functions in. If not set, no
    /// namespace will be used.
    pub fn set_namespace(&mut self, namespace: &str) {
        self.namespace = Some(namespace.to_string());
    }

    /// Sets the type that will be wrapped around the generated C# script. If not set, no type
    /// will be used.
    pub fn set_type(&mut self, type_name: &str) {
        self.type_name = Some(type_name.to_string());
    }

    /// Adds a using to the top of the C# script.
    pub fn add_using(&mut self, using: &str) {
        self.usings.push(using.to_string());
    }

    pub(crate) fn add_known_type(&self, rust_type_name: &str, csharp_type_name: &str) {
        self.configuration.borrow_mut().add_known_type(
            rust_type_name,
            self.namespace.clone(),
            self.type_name.clone(),
            csharp_type_name.to_string(),
        );
    }
}

#[derive(Debug)]
pub enum Error {
    ParseError(syn::Error),
    IOError(std::io::Error),
    FmtError(std::fmt::Error),
    UnsupportedError(String),
    UnknownType(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::ParseError(e) => e.fmt(f),
            Error::IOError(e) => e.fmt(f),
            Error::FmtError(e) => e.fmt(f),
            Error::UnsupportedError(e) => f.write_str(e),
            Error::UnknownType(e) => f.write_str(e),
        }
    }
}

impl From<syn::Error> for Error {
    fn from(error: syn::Error) -> Self {
        Error::ParseError(error)
    }
}
impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Error::IOError(error)
    }
}
impl From<std::fmt::Error> for Error {
    fn from(error: std::fmt::Error) -> Self {
        Error::FmtError(error)
    }
}
