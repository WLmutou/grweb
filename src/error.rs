use serde::Serialize;
use std::fmt;

use crate::Response;

#[derive(Debug)]
pub enum Error {
    Business {
        code: i32,
        message: String,
        details: Option<String>,
    },
    Validation {
        field: String,
        message: String,
    },
    NotFound {
        resource: String,
        id: String,
    },
    Unauthorized {
        message: String,
    },
    Forbidden {
        message: String,
    },
    Database {
        message: String,
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
    Internal {
        message: String,
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
    Custom {
        code: i32,
        message: String,
        status: u16,
        details: Option<String>,
    },
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

impl Error {
    pub fn business(code: i32, message: impl Into<String>) -> Self {
        Error::Business {
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn business_with_details(code: i32, message: impl Into<String>, details: impl Into<String>) -> Self {
        Error::Business {
            code,
            message: message.into(),
            details: Some(details.into()),
        }
    }

    pub fn validation(field: impl Into<String>, message: impl Into<String>) -> Self {
        Error::Validation {
            field: field.into(),
            message: message.into(),
        }
    }

    pub fn not_found(resource: impl Into<String>, id: impl Into<String>) -> Self {
        Error::NotFound {
            resource: resource.into(),
            id: id.into(),
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Error::Unauthorized {
            message: message.into(),
        }
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Error::Forbidden {
            message: message.into(),
        }
    }

    pub fn database(message: impl Into<String>) -> Self {
        Error::Database {
            message: message.into(),
            source: None,
        }
    }

    pub fn database_with_source(message: impl Into<String>, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Error::Database {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Error::Internal {
            message: message.into(),
            source: None,
        }
    }

    pub fn internal_with_source(message: impl Into<String>, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Error::Internal {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    pub fn custom(code: i32, message: impl Into<String>, status: u16) -> Self {
        Error::Custom {
            code,
            message: message.into(),
            status,
            details: None,
        }
    }

    pub fn custom_with_details(code: i32, message: impl Into<String>, status: u16, details: impl Into<String>) -> Self {
        Error::Custom {
            code,
            message: message.into(),
            status,
            details: Some(details.into()),
        }
    }

    pub fn status_code(&self) -> u16 {
        match self {
            Error::Business { .. } => 200,
            Error::Validation { .. } => 400,
            Error::NotFound { .. } => 404,
            Error::Unauthorized { .. } => 401,
            Error::Forbidden { .. } => 403,
            Error::Database { .. } => 500,
            Error::Internal { .. } => 500,
            Error::Custom { status, .. } => *status,
        }
    }

    pub fn error_code(&self) -> i32 {
        match self {
            Error::Business { code, .. } => *code,
            Error::Validation { .. } => 40001,
            Error::NotFound { .. } => 40401,
            Error::Unauthorized { .. } => 40101,
            Error::Forbidden { .. } => 40301,
            Error::Database { .. } => 50001,
            Error::Internal { .. } => 50002,
            Error::Custom { code, .. } => *code,
        }
    }

    pub fn error_message(&self) -> String {
        match self {
            Error::Business { message, .. } => message.clone(),
            Error::Validation { field, message } => format!("{}: {}", field, message),
            Error::NotFound { resource, id } => format!("{} not found: {}", resource, id),
            Error::Unauthorized { message } => message.clone(),
            Error::Forbidden { message } => message.clone(),
            Error::Database { message, .. } => message.clone(),
            Error::Internal { message, .. } => message.clone(),
            Error::Custom { message, .. } => message.clone(),
        }
    }

    pub fn error_details(&self) -> Option<String> {
        match self {
            Error::Business { details, .. } => details.clone(),
            Error::Custom { details, .. } => details.clone(),
            _ => None,
        }
    }

    pub fn to_response(&self) -> Response {
        let error_response = ErrorResponse {
            code: self.error_code(),
            message: self.error_message(),
            details: self.error_details(),
            field: match self {
                Error::Validation { field, .. } => Some(field.clone()),
                _ => None,
            },
        };

        Response::json_with_status(self.status_code(), error_response)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Business { message, .. } => write!(f, "Business error: {}", message),
            Error::Validation { field, message } => write!(f, "Validation error on '{}': {}", field, message),
            Error::NotFound { resource, id } => write!(f, "Not found: {} with id '{}'", resource, id),
            Error::Unauthorized { message } => write!(f, "Unauthorized: {}", message),
            Error::Forbidden { message } => write!(f, "Forbidden: {}", message),
            Error::Database { message, .. } => write!(f, "Database error: {}", message),
            Error::Internal { message, .. } => write!(f, "Internal error: {}", message),
            Error::Custom { message, .. } => write!(f, "Custom error: {}", message),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Database { source, .. } => source.as_ref().map(|e| e.as_ref() as &(dyn std::error::Error + 'static)),
            Error::Internal { source, .. } => source.as_ref().map(|e| e.as_ref() as &(dyn std::error::Error + 'static)),
            _ => None,
        }
    }
}

impl From<grorm::Error> for Error {
    fn from(err: grorm::Error) -> Self {
        Error::database_with_source("Database operation failed".to_string(), err)
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::internal_with_source("IO operation failed".to_string(), err)
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::internal_with_source("JSON serialization/deserialization failed".to_string(), err)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
