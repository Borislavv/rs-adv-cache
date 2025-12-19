//! HTTP response templates.

/// Unavailable response body bytes.
pub const UNAVAILABLE_RESPONSE_BODY: &[u8] = b"{
  \"status\": 503,
  \"error\": \"Service Unavailable\",
  \"message\": \"Sorry for that, please try again later or contact support.\"
}";

/// Internal server error response body bytes.
#[allow(dead_code)]
pub const INTERNAL_SERVER_ERROR_RESPONSE_BODY: &[u8] = b"{
  \"status\": 500,
  \"error\": \"Internal Server Error\",
  \"message\": \"Something went wrong. Please contact support immediately.\"
}";
