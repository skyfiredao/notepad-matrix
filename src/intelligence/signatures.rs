use std::ops::Range;

use super::{IntelligenceCatalog, IntelligenceError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallSignature {
    pub label: String,
    pub description: String,
    pub parameters: Vec<Range<usize>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignatureHelp {
    pub function_name: String,
    pub call_open_byte: usize,
    pub active_parameter: usize,
    pub active_overload: usize,
    pub signatures: Vec<CallSignature>,
    pub active_parameter_range: Option<Range<usize>>,
}

impl SignatureHelp {
    pub fn cycle_overload(&mut self, forward: bool) {
        if self.signatures.is_empty() {
            return;
        }
        self.active_overload = if forward {
            (self.active_overload + 1) % self.signatures.len()
        } else {
            (self.active_overload + self.signatures.len() - 1) % self.signatures.len()
        };
        self.refresh_parameter();
    }

    pub(crate) fn refresh_parameter(&mut self) {
        if self.signatures.is_empty() { return; }
        if self.active_parameter > self.signatures[self.active_overload].parameters.len()
            && let Some(index) = self.signatures.iter().position(|signature| self.active_parameter <= signature.parameters.len()) {
            self.active_overload = index;
        }
        self.active_parameter_range = self.signatures[self.active_overload]
            .parameters
            .get(self.active_parameter)
            .cloned();
    }
}

#[derive(Clone, Copy, Default)]
struct FunctionValues {
    identifier: Option<usize>,
    function: Option<usize>,
    parameter: usize,
    open: usize,
}

struct Token {
    range: Range<usize>,
    identifier: bool,
}

impl IntelligenceCatalog {
    pub fn signature_help(
        &self,
        source_key: &str,
        text: &crate::byte_buffer::ByteBuffer,
        caret: usize,
        cache: &mut Option<SignatureHelp>,
    ) -> Result<Option<SignatureHelp>, IntelligenceError> {
        if caret > text.len() { return Err(IntelligenceError("The caret is outside the document".into())); }
        let Some(api) = self.try_api(source_key)? else {
            return Ok(None);
        };
        let env = &api.environment;
        let row = text.line_index(caret).map_err(|error| IntelligenceError(error.to_string()))?;
        let start = text.line_start(row).map_err(|error| IntelligenceError(error.to_string()))?;
        let mut end = start;
        while end < text.len() && !text.is_line_end(end) { end += 1; }
        let offset = caret - start;
        if offset < 2 || end - start + 3 >= 256 { *cache = None; return Ok(None); }
        let input = text.slice(start..caret).map_err(|error| IntelligenceError(error.to_string()))?.to_vec();
        let identifier = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_' || env.additional_word_chars.as_bytes().contains(&byte);
        let mut tokens = Vec::new();
        let mut at = 0;
        while at < input.len() {
            let byte = input[at];
            if identifier(byte) {
                let begin = at;
                while at < input.len() && identifier(input[at]) { at += 1; }
                tokens.push(Token { range: begin..at, identifier: true });
            } else if matches!(byte, b' ' | b'\t' | b'\r' | b'\n') {
                at += 1;
            } else if matches!(byte, b'\'' | b'"') {
                at += 1;
                while at < input.len() && input[at] != byte {
                    if input[at] == b'\\' && at + 1 < input.len() { at += 1; }
                    at += 1;
                }
                at += usize::from(at < input.len());
            } else {
                tokens.push(Token { range: at..at + 1, identifier: false });
                at += 1;
            }
        }
        let mut values = FunctionValues::default();
        let mut stack = Vec::new();
        for (index, token) in tokens.iter().enumerate() {
            if token.identifier {
                values.identifier = Some(index);
                continue;
            }
            let byte = input[token.range.start];
            if u32::from(byte) == u32::from(env.start_function) {
                stack.push(values);
                values.open = start + token.range.start;
                if index > 0 && values.identifier == Some(index - 1) {
                    values.function = values.identifier;
                    values.parameter = 0;
                } else { values.function = None; }
            } else if u32::from(byte) == u32::from(env.parameter_separator) && values.function.is_some() {
                values.parameter += 1;
            } else if u32::from(byte) == u32::from(env.stop_function) {
                values = stack.pop().unwrap_or_default();
            } else if env.terminal.is_some_and(|terminal| u32::from(byte) == u32::from(terminal)) {
                stack.clear();
                values = FunctionValues::default();
            }
        }
        while values.function.is_none() {
            let Some(previous) = stack.pop() else { return Ok(None); };
            values = previous;
        }
        let Some(function) = values.function else { return Ok(None); };
        let name = &input[tokens[function].range.clone()];
        if let Some(cached) = cache.as_mut()
            && name.get(..cached.function_name.len()).is_some_and(|name| {
                if env.ignore_case { cached.function_name.as_bytes().eq_ignore_ascii_case(name) }
                else { cached.function_name.as_bytes() == name }
            }) {
            cached.call_open_byte = values.open;
            cached.active_parameter = values.parameter;
            cached.refresh_parameter();
            return Ok(Some(cached.clone()));
        }
        *cache = None;
        let Some(keyword) = api.keywords.iter().find(|keyword| {
            if env.ignore_case { keyword.name.as_bytes().eq_ignore_ascii_case(name) }
            else { keyword.name.as_bytes() == name }
        }) else {
            return Ok(None);
        };
        if !keyword.is_function || keyword.overloads.is_empty() {
            return Ok(None);
        }
        let signatures = keyword
            .overloads
            .iter()
            .map(|overload| {
                let mut label = String::new();
                label.push_str(&overload.return_value);
                label.push(' ');
                label.push_str(&String::from_utf8_lossy(name));
                label.push(' ');
                label.push(env.start_function);
                let mut parameters = Vec::new();
                for (index, parameter) in overload.parameters.iter().enumerate() {
                    if index > 0 {
                        label.push(env.parameter_separator);
                        label.push(' ');
                    }
                    let start = label.len();
                    label.push_str(&parameter.name);
                    parameters.push(start..label.len());
                }
                label.push(env.stop_function);
                CallSignature {
                    label,
                    description: overload.description.clone(),
                    parameters,
                }
            })
            .collect::<Vec<_>>();
        let active_parameter_range = signatures[0]
            .parameters
            .first()
            .cloned();
        let help = SignatureHelp {
            function_name: String::from_utf8_lossy(name).into_owned(),
            call_open_byte: values.open,
            active_parameter: 0,
            active_overload: 0,
            signatures,
            active_parameter_range,
        };
        *cache = Some(help.clone());
        Ok(Some(help))
    }
}
