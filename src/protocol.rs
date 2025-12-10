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
