use std::{collections::HashMap, fmt::Display};

use crate::header::Header;

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
}

pub enum Protocol {
    Http1_1,
    Http1_0,
    Http0_9,
    Missing,
}

impl<'a> TryFrom<Option<&'a str>> for Protocol {
    type Error = &'a str;

    fn try_from(value: Option<&'a str>) -> Result<Self, Self::Error> {
        Ok(match value {
            Some(string) => match string {
                "HTTP/1.1" => Self::Http1_1,
                "HTTP/1.0" => Self::Http1_0,
                "HTTP/0.9" => Self::Http0_9,
                _ => return Err(string),
            },
            None => Self::Missing,
        })
    }
}

impl Protocol {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Http1_1 => "HTTP/1.1",
            Self::Http1_0 => "HTTP/1.0",
            Self::Http0_9 => "HTTP/0.9",
            Self::Missing => "",
        }
    }
}

#[derive(Debug, Clone)]
pub enum RequestParseError {
    RequestLineMissing,
    MethodMissing,
    UnknownMethod(String),
    TargetMissing,
    UnknownProtocol(String),
}

pub struct Request {
    method: Method,
    target: String,
    protocol: Protocol,
    header_fields: HashMap<Header, String>,
}

impl TryFrom<&str> for Request {
    type Error = RequestParseError;

    fn try_from(string: &str) -> Result<Self, RequestParseError> {
        let mut lines = string.lines();
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
        Ok(Self {
            method,
            target,
            protocol,
            header_fields,
        })
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
        write!(f, "\n\n")?;
        Ok(())
    }
}
