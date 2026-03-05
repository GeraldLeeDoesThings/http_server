use std::{collections::HashMap, fmt::Display};

use crate::{header::Header, protocol::Protocol};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    Get,
    Head,
    Post,
    Put,
    Delete,
    Connect,
    Options,
    Trace,
    Patch,
}

pub const METHODS: [Method; 9] = [
    Method::Get,
    Method::Head,
    Method::Post,
    Method::Put,
    Method::Delete,
    Method::Connect,
    Method::Options,
    Method::Trace,
    Method::Patch,
];

impl<'a> TryFrom<&'a str> for Method {
    type Error = &'a str;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        Ok(match value {
            "GET" => Self::Get,
            "HEAD" => Self::Head,
            "POST" => Self::Post,
            "PUT" => Self::Put,
            "DELETE" => Self::Delete,
            "CONNECT" => Self::Connect,
            "OPTIONS" => Self::Options,
            "TRACE" => Self::Trace,
            "PATCH" => Self::Patch,
            _ => return Err(value),
        })
    }
}

impl Method {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Head => "HEAD",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
            Self::Connect => "CONNECT",
            Self::Options => "OPTIONS",
            Self::Trace => "TRACE",
            Self::Patch => "PATCH",
        }
    }

    pub const fn index(&self) -> usize {
        *self as usize
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestParseError {
    ContentDelimiterMissing,
    RequestLineMissing,
    MethodMissing,
    UnknownMethod(String),
    TargetMissing,
    UnknownProtocol(String),
    ContentLengthMissing,
    ContentLengthMalformed,
    ContentLengthIncorrect,
    RequestIncomplete,
}

#[derive(Debug)]
pub struct Request {
    method: Method,
    target: String,
    protocol: Protocol,
    header_fields: HashMap<Header, String>,
    path_parameters: HashMap<String, String>,
    content: String,
}

impl Request {
    pub const fn get_target(&self) -> &String {
        &self.target
    }

    pub const fn get_protocol(&self) -> Protocol {
        self.protocol
    }

    pub const fn get_path_parameters(&self) -> &HashMap<String, String> {
        &self.path_parameters
    }

    pub const fn get_path_parameters_mut(&mut self) -> &mut HashMap<String, String> {
        &mut self.path_parameters
    }

    pub fn set_header(&mut self, header: Header, field: String) -> Option<String> {
        self.header_fields.insert(header, field)
    }

    pub const fn get_content(&self) -> &String {
        &self.content
    }

    pub const fn get_method(&self) -> &Method {
        &self.method
    }

    pub fn new(
        method: Method,
        target: String,
        protocol: Protocol,
        header_fields: HashMap<Header, String>,
        path_parameters: HashMap<String, String>,
        content: String,
    ) -> Result<Self, RequestParseError> {
        if let Some(length_str) = header_fields.get(&Header::ContentLength) {
            let length: usize = length_str
                .parse()
                .map_err(|_| RequestParseError::ContentLengthMalformed)?;
            if content.len() != length {
                Err(RequestParseError::ContentLengthIncorrect)
            } else {
                Ok(Self {
                    method,
                    target,
                    protocol,
                    header_fields,
                    path_parameters,
                    content,
                })
            }
        } else {
            Err(RequestParseError::ContentLengthMissing)
        }
    }
}

impl TryFrom<&str> for Request {
    type Error = RequestParseError;

    fn try_from(string: &str) -> Result<Self, RequestParseError> {
        let (headers, content) = string
            .split_once("\r\n\r\n")
            .ok_or(RequestParseError::ContentDelimiterMissing)?;
        let mut lines = headers.lines();
        let mut request_line_parts = lines
            .next()
            .ok_or(RequestParseError::RequestLineMissing)?
            .split_whitespace();
        let method: Method = request_line_parts
            .next()
            .ok_or(RequestParseError::MethodMissing)?
            .try_into()
            .map_err(|err: &str| RequestParseError::UnknownMethod(err.to_string()))?;
        let target: String = request_line_parts
            .next()
            .ok_or(RequestParseError::TargetMissing)?
            .to_string();
        let protocol: Protocol = request_line_parts
            .next()
            .try_into()
            .map_err(|err: &str| RequestParseError::UnknownProtocol(err.to_string()))?;
        let header_fields: HashMap<Header, String> =
            HashMap::from_iter(lines.map_while(|line| line.split_once(':')).map(
                |(raw_header, raw_field)| (raw_header.into(), raw_field.trim_start().to_string()),
            ));
        Self::new(
            method,
            target,
            protocol,
            header_fields,
            HashMap::new(),
            content.to_string(),
        )
    }
}

impl Display for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "{} {} {}",
            self.method.as_str(),
            self.target,
            self.protocol.as_str()
        )?;
        for (header, field) in &self.header_fields {
            writeln!(f, "{}: {}", header.as_str(), field)?;
        }
        write!(f, "\r\n{}", self.content)?;
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum RequestFactoryState {
    ParsingHeaders,
    ParsingHeaderStop,
    ParsingBody(usize),
    Completed,
    Failed(RequestParseError),
}

#[derive(Debug)]
pub struct RequestFactory {
    method: Option<Method>,
    target: Option<String>,
    protocol: Option<Protocol>,
    state: RequestFactoryState,
    buffer: String,
    header_fields: Option<HashMap<Header, String>>,
    content: String,
}

impl Default for RequestFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestFactory {
    pub const fn new() -> Self {
        Self {
            method: None,
            target: None,
            protocol: None,
            state: RequestFactoryState::ParsingHeaders,
            buffer: String::new(),
            header_fields: None,
            content: String::new(),
        }
    }

    fn finalize_headers(&mut self) -> Result<(), RequestParseError> {
        assert_eq!(self.state, RequestFactoryState::ParsingHeaderStop);
        let mut lines = self.buffer.lines();
        let mut request_line_parts = lines
            .next()
            .ok_or(RequestParseError::RequestLineMissing)?
            .split_whitespace();
        let method: Method = request_line_parts
            .next()
            .ok_or(RequestParseError::MethodMissing)?
            .try_into()
            .map_err(|err: &str| RequestParseError::UnknownMethod(err.to_string()))?;
        let target: String = request_line_parts
            .next()
            .ok_or(RequestParseError::TargetMissing)?
            .to_string();
        let protocol: Protocol = request_line_parts
            .next()
            .try_into()
            .map_err(|err: &str| RequestParseError::UnknownProtocol(err.to_string()))?;
        let header_fields: HashMap<Header, String> =
            HashMap::from_iter(lines.map_while(|line| line.split_once(':')).map(
                |(raw_header, raw_field)| (raw_header.into(), raw_field.trim_start().to_string()),
            ));
        assert!(
            self.method.replace(method).is_none(),
            "Parsed method for request, but a method was already present."
        );
        assert!(
            self.target.replace(target).is_none(),
            "Parsed target for request, but a target was already present."
        );
        assert!(
            self.protocol.replace(protocol).is_none(),
            "Parsed protocol for request, but a protocol was already present."
        );
        assert!(
            self.header_fields.replace(header_fields).is_none(),
            "Parsed headers for request, but headers were already present."
        );
        let length: usize = self
            .header_fields
            .as_ref()
            .expect("Just assigned headers, but none were found.")
            .get(&Header::ContentLength)
            .ok_or(RequestParseError::ContentLengthMissing)?
            .parse()
            .map_err(|_| RequestParseError::ContentLengthMalformed)?;
        self.buffer.clear();
        if length == 0 {
            self.state = RequestFactoryState::Completed;
        } else {
            self.state = RequestFactoryState::ParsingBody(length);
        }
        Ok(())
    }

    pub const fn is_completed(&self) -> bool {
        matches!(self.state, RequestFactoryState::Completed)
    }

    pub fn process_str(&mut self, str: &str) -> Result<(), RequestParseError> {
        match &mut self.state {
            RequestFactoryState::ParsingHeaders | RequestFactoryState::ParsingHeaderStop => {
                for (index, c) in str.char_indices() {
                    match c {
                        '\n' => match self.state {
                            RequestFactoryState::ParsingHeaderStop => {
                                self.buffer.push_str(&str[0..=index]);
                                self.finalize_headers()?;
                                if str.len() > index {
                                    return self.process_str(&str[index + 1..]);
                                }
                            }
                            RequestFactoryState::ParsingHeaders => {
                                self.state = RequestFactoryState::ParsingHeaderStop
                            }
                            _ => unreachable!(),
                        },
                        '\r' => {}
                        _ => self.state = RequestFactoryState::ParsingHeaders,
                    }
                }
                if matches!(
                    self.state,
                    RequestFactoryState::ParsingHeaders | RequestFactoryState::ParsingHeaderStop
                ) {
                    self.buffer.push_str(str);
                }
                Ok(())
            }
            RequestFactoryState::ParsingBody(remaining_bytes) => {
                let remaining_bytes = *remaining_bytes;
                let slice = &str[0..remaining_bytes.min(str.len())];
                self.content.push_str(slice);
                let remaining = remaining_bytes - slice.len();
                assert!(remaining <= remaining_bytes);
                if remaining == 0 {
                    self.state = RequestFactoryState::Completed;
                } else {
                    self.state = RequestFactoryState::ParsingBody(remaining);
                }
                Ok(())
            }
            RequestFactoryState::Completed => Ok(()),
            RequestFactoryState::Failed(error) => Err(error.clone()),
        }
    }

    pub fn get_state(&self) -> RequestFactoryState {
        self.state.clone()
    }
}

impl TryInto<Request> for RequestFactory {
    type Error = RequestParseError;

    fn try_into(self) -> Result<Request, Self::Error> {
        match self.state {
            RequestFactoryState::Completed => Ok(Request {
                method: self
                    .method
                    .expect("State is completed, but method is missing."),
                target: self
                    .target
                    .expect("State is completed, but target is missing."),
                protocol: self
                    .protocol
                    .expect("State is completed, but protocol is missing."),
                header_fields: self
                    .header_fields
                    .expect("State is completed, but headers are missing."),
                path_parameters: HashMap::new(),
                content: self.content,
            }),
            RequestFactoryState::Failed(error) => Err(error),
            _ => Err(RequestParseError::RequestIncomplete),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_factory_single_transition() {
        let mut factory = RequestFactory::new();
        assert_eq!(factory.state, RequestFactoryState::ParsingHeaders);
        assert!(factory.process_str("\r\n").is_ok());
        assert_eq!(factory.state, RequestFactoryState::ParsingHeaderStop);
    }

    #[test]
    fn request_factory_breakout_blank() {
        let mut factory = RequestFactory::new();
        assert_eq!(factory.state, RequestFactoryState::ParsingHeaders);
        assert_eq!(
            factory.process_str("\r\n\r\n"),
            Err(RequestParseError::MethodMissing)
        );
        assert_eq!(factory.state, RequestFactoryState::ParsingHeaderStop);
    }

    #[test]
    fn request_factory_no_length() {
        let mut factory = RequestFactory::new();
        assert_eq!(factory.state, RequestFactoryState::ParsingHeaders);
        assert_eq!(
            factory.process_str("GET /test HTTP/1.1\r\n\r\n"),
            Err(RequestParseError::ContentLengthMissing)
        );
        assert_eq!(factory.state, RequestFactoryState::ParsingHeaderStop);
    }

    #[test]
    fn request_factory_no_content() {
        let mut factory = RequestFactory::new();
        assert_eq!(factory.state, RequestFactoryState::ParsingHeaders);
        assert!(
            factory
                .process_str("GET /test HTTP/1.1\r\nContent-Length: 0\r\n\r\n")
                .is_ok()
        );
        assert_eq!(factory.state, RequestFactoryState::Completed);
    }

    #[test]
    fn request_factory_content_missing() {
        let mut factory = RequestFactory::new();
        assert_eq!(factory.state, RequestFactoryState::ParsingHeaders);
        assert!(
            factory
                .process_str("GET /test HTTP/1.1\r\nContent-Length: 5\r\n\r\n")
                .is_ok()
        );
        assert_eq!(factory.state, RequestFactoryState::ParsingBody(5));
    }

    #[test]
    fn request_factory_newline_content() {
        let mut factory = RequestFactory::new();
        assert_eq!(factory.state, RequestFactoryState::ParsingHeaders);
        assert!(
            factory
                .process_str("GET /test HTTP/1.1\r\nContent-Length: 1\r\n\r\n\n")
                .is_ok()
        );
        assert_eq!(factory.state, RequestFactoryState::Completed);
    }

    #[test]
    fn request_factory_content() {
        let mut factory = RequestFactory::new();
        assert_eq!(factory.state, RequestFactoryState::ParsingHeaders);
        assert!(
            factory
                .process_str("GET /test HTTP/1.1\r\nContent-Length: 5\r\n\r\nhello")
                .is_ok()
        );
        assert_eq!(factory.state, RequestFactoryState::Completed);
        assert_eq!(factory.content, "hello");
    }
}
