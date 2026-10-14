// SPDX-License-Identifier: AGPL-3.0-or-later
// Part of NihAV by Kostya Shishkov (https://nihav.org), trimmed; see vendor/README.md and COPYING.
//! Options support.
//!
//! This module contains the definitions for options and names for common options.
//! Options are used to set custom parameters in e.g. decoders or muxers.
//!
//! As a rule target for options should provide a list of supported options and ignore unknown options.

use std::sync::Arc;


/// Option definition type.
#[derive(Debug)]
pub enum NAOptionDefinitionType {
    /// Option may just be present.
    None,
    /// Option is a boolean value.
    Bool,
    /// Option is an integer with optional minimum and maximum value.
    Int(Option<i64>, Option<i64>),
    /// Option is a floating point number with optional minimum and maximum value.
    Float(Option<f64>, Option<f64>),
    /// Option is a string with an optional list of allowed values.
    String(Option<&'static [&'static str]>),
    /// Option is some binary data.
    Data,
}

/// Option definition.
#[derive(Debug)]
pub struct NAOptionDefinition {
    /// Option name.
    pub name:           &'static str,
    /// Option meaning.
    pub description:    &'static str,
    /// Option type.
    pub opt_type:       NAOptionDefinitionType,
}


/// Option.
#[derive(Clone,Debug,PartialEq)]
pub struct NAOption {
    /// Option name.
    pub name:   &'static str,
    /// Option value.
    pub value:  NAValue,
}

/// A list of accepted option values.
#[derive(Debug,Clone,PartialEq)]
pub enum NAValue {
    /// Empty value.
    None,
    /// Boolean value.
    Bool(bool),
    /// Integer value.
    Int(i64),
    /// Floating point value.
    Float(f64),
    /// String value.
    String(String),
    /// Binary data value.
    Data(Arc<Vec<u8>>),
}

/// Trait for all objects that handle `NAOption`.
pub trait NAOptionHandler {
    /// Returns the options recognised by current object.
    fn get_supported_options(&self) -> &[NAOptionDefinition];
    /// Passes options for the object to set (or ignore).
    fn set_options(&mut self, options: &[NAOption]);
    /// Queries the current option value in the object (if present).
    fn query_option_value(&self, name: &str) -> Option<NAValue>;
}
