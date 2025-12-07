#[derive(PartialEq, Eq, Hash)]
pub enum Header {
    ContentLength,
    ContentType,
    ContentEncoding,
    ContentLanguage,
    ContentLocation,

    From,
    Host,
    Referer,
    ReferrerPolicy,
    UserAgent,

    Other(String),
}

impl From<&str> for Header {
    fn from(value: &str) -> Self {
        match value.to_lowercase().as_str() {
            "content-length" => Self::ContentLength,
            "content-type" => Self::ContentType,
            "content-encoding" => Self::ContentEncoding,
            "content-language" => Self::ContentLanguage,
            "content-location" => Self::ContentLocation,

            "from" => Self::From,
            "host" => Self::Host,
            "referer" => Self::Referer,
            "referrer-policy" => Self::ReferrerPolicy,
            "user-agent" => Self::UserAgent,

            _ => Self::Other(value.to_string()),
        }
    }
}

impl<'a> Header {
    pub const fn as_str(&'a self) -> &'a str {
        match self {
            Self::ContentLength => "Content-Length",
            Self::ContentType => "Content-Type",
            Self::ContentEncoding => "Content-Encoding",
            Self::ContentLanguage => "Content-Language",
            Self::ContentLocation => "Content-Location",
            Self::From => "From",
            Self::Host => "Host",
            Self::Referer => "Referer",
            Self::ReferrerPolicy => "Referrer-Policy",
            Self::UserAgent => "User-Agent",
            Self::Other(header) => header.as_str(),
        }
    }
}
