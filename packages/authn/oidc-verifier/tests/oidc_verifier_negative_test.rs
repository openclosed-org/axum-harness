use authn_oidc_verifier::{OidcError, OidcVerifier, OidcVerifierConfig};
use base64::Engine;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use mockito::{Server, ServerGuard};

const TEST_RSA_PRIVATE_KEY: &str = "-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQDPgCpR4svx5LYk\nyK6M7jAGNu8CtM7Rlq12pnog2ePTZPdj+E9tfhaIHHR9HZgaVfZyfRFcdA4yBN6O\nyOdgN7hSlANEJeUhBRIetkvzGOEpvLiPGisGc1DkYeA4bzTUPizbL77amz2STbMq\n1dSNptv9sr7dB+VSpEfUV66L9Kjvm6x28YqwDDfVwE8mxXzIR3cZOXFJTM/nYhf3\nUZ8oYfqGL8JmCTqai/nkOcpM7qj8CWrtXneSn+FRYMM/TnF3E4tlRYTg/FVOog5+\n8DfIjw9Thf8y8XbVKJPQJt0UCxc/a5Hb/7UTxDl5MVN68BCEvLdbHIHjMV61bn3v\nQUwYVkulAgMBAAECggEAFdAx4rjWWsAB290S6HLTrpuQxbaPNV5DLwFyPkjZl+v5\ny9MbOnXyVW20W0DEsCQQS9nU/OSgZ2a2pMj+9dD1ugygSUY4j5+SV5MvacdYSER0\nHGsSUdPGkbOuWBBsu9ErcwFSbXW7Y8lyR9MBzMBZSRLE2MSPOYBWor5y9XiLV+DT\nky5ri59rRpK8suJLmbvl8PyY6LwC9mrsXAFffe1rJLnFRR3grthXTramvksWPi4T\njUYYCZFEQphZqoMm2/ffwF/22QDACpC/aqxqhdep+I9JPDY13jSLStyKHb7LWwBb\nCZGR2BDP82BZAcbUeQXud1A6LIo35Arn+luomDOWcQKBgQD1cu10c3+130GxZJD4\nd9C4Ou5rUhNbzrICoQuefHiEpQMngOffS9Pl3A3UnIQWrddjG4cUlVcefbNzzljA\nGishXSegzMT0J8CaVtRgFQhw0xWjJnBA+q4zmpU/6SaqkyitewvsqOtMwSOVBPXr\n3v1rPf9gs4SrcTiQWF4L4acfDQKBgQDYa6GXVyMAVmYeeCsGCCLH0+MrqzO+D+K4\nKn4hP3g8x+23HufObupibupEGDRzFTOoKXka8984XNw2a6Jjrf8cC+FaL9bz7sxs\nSDgv1JbrfBm2+Xta4m7gIRBXlCCGGLe3JETwbnbmusFQd/Paq90BasRNzZTdsPFH\nJiBu6Gd4+QKBgC9vyMiq0dHalh2sq//5WBNjAFUphahGqEytx0sYD0rDgXqPBUE4\nrHlOMDYZEcY4TtpOpaqqui2gaaBGDw0BgbhvAounR6FQVX7+rQjsx7bWdOYVNbi5\nOhWrGJFDhD+PNVth3oock21AHppcXRL7A8tILiUITOm9dgsfqP1u3Re5AoGALuAp\nMPGDuEf+eG0IzJaoieXAF65OV8VzEvbJOQRZU7juKTK9fL4TcFyby0H+4kpeVPce\nrxLRb5DVdcgcdUCzt+xu1Cz2fwFjL7T4zotaYQkRPMuOx2GyKEOhGYcRAFqMOFPX\nxsf2YwViZ76DiAKfrPXmLP/xVY9Ew2djsQIPn2kCgYEAhhR9JAqyruqOjCobZsBy\n9NCOYY8g6qmfdwcKDPslXa43d24RYJXXk33b51EQLgHT7/1jB+Y87GgBHLNsLk4Q\njF1oOS7bOdeloMElpj8oqDTIScrC1lT6y/bE4Wn3Kv8nRBegLZCtHVRll8yejwn7\nAsDT6h7HYx8KBJmjJnRTBBo=\n-----END PRIVATE KEY-----\n";

const TEST_JWKS: &str = r#"{
  "keys": [
    {
      "kty": "RSA",
      "kid": "oidc-test-key",
      "alg": "RS256",
      "use": "sig",
      "n": "z4AqUeLL8eS2JMiujO4wBjbvArTO0ZatdqZ6INnj02T3Y_hPbX4WiBx0fR2YGlX2cn0RXHQOMgTejsjnYDe4UpQDRCXlIQUSHrZL8xjhKby4jxorBnNQ5GHgOG801D4s2y--2ps9kk2zKtXUjabb_bK-3QflUqRH1Feui_So75usdvGKsAw31cBPJsV8yEd3GTlxSUzP52IX91GfKGH6hi_CZgk6mov55DnKTO6o_Alq7V53kp_hUWDDP05xdxOLZUWE4PxVTqIOfvA3yI8PU4X_MvF21SiT0CbdFAsXPuR2_-1E8Q5eTFTevAQhLy3WxyB4zFetW5970FMGFZLpQ",
      "e": "AQAB"
    }
  ]
}"#;

#[derive(serde::Serialize)]
struct Claims {
    sub: String,
    exp: usize,
    iss: String,
    aud: String,
}

async fn test_verifier(audience: &str) -> (ServerGuard, OidcVerifier, String) {
    let mut server = Server::new_async().await;
    let issuer = server.url();
    let jwks_uri = format!("{issuer}/oauth/v2/keys");

    server
        .mock("GET", "/.well-known/openid-configuration")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(format!(r#"{{"jwks_uri":"{jwks_uri}"}}"#))
        .create_async()
        .await;
    server
        .mock("GET", "/oauth/v2/keys")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(TEST_JWKS)
        .create_async()
        .await;

    let verifier = OidcVerifier::new(
        OidcVerifierConfig {
            issuer: issuer.clone(),
            audience: audience.to_string(),
            introspection_url: String::new(),
            introspection_client_id: String::new(),
            introspection_client_secret: String::new(),
        },
        reqwest::Client::new(),
    );

    (server, verifier, issuer)
}

fn make_rs256_token(sub: &str, issuer: &str, audience: &str, kid: &str, exp: usize) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(kid.to_string());

    encode(
        &header,
        &Claims {
            sub: sub.to_string(),
            exp,
            iss: issuer.to_string(),
            aud: audience.to_string(),
        },
        &EncodingKey::from_rsa_pem(TEST_RSA_PRIVATE_KEY.as_bytes()).unwrap(),
    )
    .unwrap()
}

fn make_hs256_token(sub: &str, issuer: &str, audience: &str) -> String {
    let mut header = Header::new(Algorithm::HS256);
    header.kid = Some("oidc-test-key".to_string());

    encode(
        &header,
        &Claims {
            sub: sub.to_string(),
            exp: 9_999_999_999,
            iss: issuer.to_string(),
            aud: audience.to_string(),
        },
        &EncodingKey::from_secret(b"not-the-jwks-key"),
    )
    .unwrap()
}

fn make_none_alg_token(issuer: &str, audience: &str) -> String {
    let header = serde_json::json!({"alg":"none","kid":"oidc-test-key","typ":"JWT"});
    let claims = serde_json::json!({
        "sub": "none-alg-user",
        "exp": 9_999_999_999usize,
        "iss": issuer,
        "aud": audience,
    });

    let engine = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    format!(
        "{}.{}.",
        engine.encode(serde_json::to_vec(&header).unwrap()),
        engine.encode(serde_json::to_vec(&claims).unwrap())
    )
}

async fn assert_unauthorized(verifier: &OidcVerifier, token: &str) {
    assert!(matches!(
        verifier.verify(token).await,
        Err(OidcError::Unauthorized)
    ));
}

#[tokio::test]
async fn rejects_jwt_with_disallowed_algorithm() {
    let (_server, verifier, issuer) = test_verifier("api://default").await;
    let token = make_hs256_token("user-a", &issuer, "api://default");

    assert_unauthorized(&verifier, &token).await;
}

#[tokio::test]
async fn rejects_jwt_with_none_algorithm() {
    let (_server, verifier, issuer) = test_verifier("api://default").await;
    let token = make_none_alg_token(&issuer, "api://default");

    assert_unauthorized(&verifier, &token).await;
}

#[tokio::test]
async fn rejects_jwt_with_unknown_kid() {
    let (_server, verifier, issuer) = test_verifier("api://default").await;
    let token = make_rs256_token(
        "user-a",
        &issuer,
        "api://default",
        "unknown-key",
        9_999_999_999,
    );

    assert_unauthorized(&verifier, &token).await;
}

#[tokio::test]
async fn rejects_jwt_with_wrong_issuer() {
    let (_server, verifier, _issuer) = test_verifier("api://default").await;
    let token = make_rs256_token(
        "user-a",
        "https://issuer.example.invalid",
        "api://default",
        "oidc-test-key",
        9_999_999_999,
    );

    assert_unauthorized(&verifier, &token).await;
}

#[tokio::test]
async fn rejects_jwt_with_wrong_audience() {
    let (_server, verifier, issuer) = test_verifier("api://default").await;
    let token = make_rs256_token(
        "user-a",
        &issuer,
        "api://wrong",
        "oidc-test-key",
        9_999_999_999,
    );

    assert_unauthorized(&verifier, &token).await;
}

#[tokio::test]
async fn rejects_malformed_jwt() {
    let (_server, verifier, _issuer) = test_verifier("api://default").await;

    assert_unauthorized(&verifier, "not-a-jwt").await;
}

#[tokio::test]
async fn rejects_expired_jwt() {
    let (_server, verifier, issuer) = test_verifier("api://default").await;
    let token = make_rs256_token("user-a", &issuer, "api://default", "oidc-test-key", 1);

    assert_unauthorized(&verifier, &token).await;
}
