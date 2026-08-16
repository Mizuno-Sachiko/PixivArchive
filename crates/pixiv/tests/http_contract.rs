use futures_util::TryStreamExt;
use pixivarchive_domain::pixiv::{
    PixivArtistFollowRequest, PixivBookmarkAddRequest, PixivBookmarkVisibility, PixivBookmarksMode,
    PixivBookmarksRequest, PixivFollowLatestMode, PixivFollowLatestRequest,
    PixivFollowLatestSource, PixivFollowingRequest, PixivFollowingVisibility, PixivRankingContent,
    PixivRankingMode, PixivRankingRequest,
};
use pixivarchive_pixiv::{
    ADAPTER_VERSION, AdapterResponse, PixivClientOptions, PixivEndpoint, PixivGateway,
    PixivMediaGateway, PixivRequestContext, PixivRequestGate, PixivWebClient,
    error::PixivErrorClass, is_official_pixiv_asset_url,
};
use secrecy::SecretString;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    thread,
    time::{Duration as StdDuration, Instant},
};
use time::{Date, Month};
use url::Url;

#[derive(Clone, Debug)]
struct CapturedRequest {
    method: String,
    target: String,
    headers: BTreeMap<String, String>,
    body: String,
    received_at: Instant,
}

struct TestServer {
    base_url: Url,
    requests: Arc<Mutex<Vec<CapturedRequest>>>,
    thread: thread::JoinHandle<()>,
}

impl TestServer {
    fn spawn(responses: Vec<Vec<u8>>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&requests);
        let thread = thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(StdDuration::from_secs(2)))
                    .unwrap();
                captured.lock().unwrap().push(read_request(&mut stream));
                stream.write_all(&response).unwrap();
            }
        });

        Self {
            base_url: Url::parse(&format!("http://{address}/")).unwrap(),
            requests,
            thread,
        }
    }

    fn finish(self) -> Vec<CapturedRequest> {
        self.thread.join().unwrap();
        Arc::try_unwrap(self.requests)
            .unwrap()
            .into_inner()
            .unwrap()
    }
}

fn read_request(stream: &mut std::net::TcpStream) -> CapturedRequest {
    let received_at = Instant::now();
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        let read = stream.read(&mut chunk).unwrap();
        assert!(read > 0, "connection closed before request headers");
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(index) = find_bytes(&buffer, b"\r\n\r\n") {
            break index + 4;
        }
    };

    let header_text = std::str::from_utf8(&buffer[..header_end]).unwrap();
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().unwrap();
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap().to_owned();
    let target = request_parts.next().unwrap().to_owned();
    let mut headers = BTreeMap::new();
    for line in lines.filter(|line| !line.is_empty()) {
        let (name, value) = line.split_once(':').unwrap();
        headers.insert(name.to_ascii_lowercase(), value.trim().to_owned());
    }

    let content_length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    while buffer.len() < header_end + content_length {
        let read = stream.read(&mut chunk).unwrap();
        assert!(read > 0, "connection closed before request body");
        buffer.extend_from_slice(&chunk[..read]);
    }

    CapturedRequest {
        method,
        target,
        headers,
        body: String::from_utf8(buffer[header_end..header_end + content_length].to_vec()).unwrap(),
        received_at,
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn response(status: u16, content_type: &str, body: impl AsRef<[u8]>) -> Vec<u8> {
    let body = body.as_ref();
    let reason = match status {
        200 => "OK",
        302 => "Found",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        429 => "Too Many Requests",
        502 => "Bad Gateway",
        _ => "Fixture",
    };
    let mut response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(body);
    response
}

fn response_with_headers(status: u16, headers: &[(&str, &str)], body: impl AsRef<[u8]>) -> Vec<u8> {
    let body = body.as_ref();
    let mut response = format!("HTTP/1.1 {status} Fixture\r\n").into_bytes();
    for (name, value) in headers {
        response.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
    }
    response.extend_from_slice(
        format!(
            "Content-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .as_bytes(),
    );
    response.extend_from_slice(body);
    response
}

fn chunked_response(status: u16, content_type: &str, body: &[u8]) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 {status} Fixture\r\nContent-Type: {content_type}\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
    )
    .into_bytes();
    for chunk in body.chunks(11) {
        response.extend_from_slice(format!("{:X}\r\n", chunk.len()).as_bytes());
        response.extend_from_slice(chunk);
        response.extend_from_slice(b"\r\n");
    }
    response.extend_from_slice(b"0\r\n\r\n");
    response
}

fn fixture(name: &str) -> Value {
    let path = format!("{}/../../fixtures/pixiv/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn context() -> PixivRequestContext {
    PixivRequestContext::new(
        SecretString::from("PHPSESSID=http-contract-cookie"),
        10001,
        "Mozilla/5.0 PixivArchive fixture",
    )
}

fn client(server: &TestServer, metadata_limit: usize, csrf_limit: usize) -> PixivWebClient {
    PixivWebClient::new(PixivClientOptions {
        web_base_url: server.base_url.clone(),
        allowed_media_hosts: BTreeSet::from([server.base_url.host_str().unwrap().to_owned()]),
        metadata_response_limit: metadata_limit,
        csrf_response_limit: csrf_limit,
        request_timeout: StdDuration::from_secs(2),
        use_system_proxy: false,
        metadata_request_gate: None,
        media_request_gate: None,
    })
    .unwrap()
}

#[tokio::test]
async fn metadata_rate_limit_counts_each_http_request() {
    let identity = json!({
        "error": false,
        "message": "",
        "body": {
            "userId": "10001",
            "name": "Test Artist"
        }
    })
    .to_string();
    let profile = fixture("profile_all.json").to_string();
    let private_shape = fixture("bookmarks.json").to_string();
    let server = TestServer::spawn(vec![
        response(200, "application/json", identity),
        response(200, "application/json", profile),
        response(200, "application/json", private_shape),
    ]);
    let gateway = PixivWebClient::new(PixivClientOptions {
        web_base_url: server.base_url.clone(),
        allowed_media_hosts: BTreeSet::from([server.base_url.host_str().unwrap().to_owned()]),
        metadata_response_limit: 64 * 1024,
        csrf_response_limit: 64 * 1024,
        request_timeout: StdDuration::from_secs(2),
        use_system_proxy: false,
        metadata_request_gate: Some(
            PixivRequestGate::new(3, Some((2, StdDuration::from_millis(100)))).unwrap(),
        ),
        media_request_gate: None,
    })
    .unwrap();

    gateway.validate_account(&context()).await.unwrap();

    let requests = server.finish();
    assert_eq!(requests.len(), 3);
    assert!(
        requests[1]
            .received_at
            .duration_since(requests[0].received_at)
            >= StdDuration::from_millis(40)
    );
    assert!(
        requests[2]
            .received_at
            .duration_since(requests[1].received_at)
            >= StdDuration::from_millis(40)
    );
}

#[tokio::test]
async fn work_detail_sends_browser_headers_and_preserves_raw_json() {
    let raw = fixture("illust.json")["illustration"].clone();
    let server = TestServer::spawn(vec![response(200, "application/json", raw.to_string())]);
    let gateway = client(&server, 64 * 1024, 64 * 1024);

    let AdapterResponse { value, provenance } =
        gateway.work_detail(&context(), 1001).await.unwrap();
    assert_eq!(value.work_id, 1001);
    assert_eq!(provenance.len(), 1);
    assert_eq!(provenance[0].adapter_version, ADAPTER_VERSION);
    assert_eq!(provenance[0].endpoint, PixivEndpoint::WorkDetail);
    assert_eq!(provenance[0].raw, raw);

    let requests = server.finish();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.method, "GET");
    assert_eq!(request.target, "/ajax/illust/1001");
    assert_eq!(
        request.headers.get("cookie").map(String::as_str),
        Some("PHPSESSID=http-contract-cookie")
    );
    assert_eq!(
        request.headers.get("user-agent").map(String::as_str),
        Some("Mozilla/5.0 PixivArchive fixture")
    );
    assert_eq!(
        request.headers.get("accept").map(String::as_str),
        Some("application/json")
    );
    assert_eq!(
        request.headers.get("referer").map(String::as_str),
        Some("https://www.pixiv.net/")
    );
}

#[tokio::test]
async fn every_supported_ranking_mode_and_content_has_a_fixed_query_value() {
    let modes = [
        (PixivRankingMode::Daily, "daily"),
        (PixivRankingMode::Weekly, "weekly"),
        (PixivRankingMode::Monthly, "monthly"),
        (PixivRankingMode::Rookie, "rookie"),
        (PixivRankingMode::Original, "original"),
        (PixivRankingMode::AiGenerated, "daily_ai"),
        (PixivRankingMode::R18, "daily_r18"),
        (PixivRankingMode::R18g, "r18g"),
        (PixivRankingMode::Male, "male"),
        (PixivRankingMode::Female, "female"),
    ];
    let contents = [
        (PixivRankingContent::All, "all"),
        (PixivRankingContent::Illustration, "illust"),
        (PixivRankingContent::Manga, "manga"),
        (PixivRankingContent::Ugoira, "ugoira"),
    ];
    let raw = fixture("ranking.json").to_string();
    let expected_pairs = modes
        .into_iter()
        .flat_map(|mode| contents.into_iter().map(move |content| (mode, content)))
        .filter(|((mode, _), (content, _))| mode.supports_content(*content))
        .collect::<Vec<_>>();
    let server = TestServer::spawn(
        expected_pairs
            .iter()
            .map(|_| response(200, "application/json", &raw))
            .collect(),
    );
    let gateway = client(&server, 64 * 1024, 64 * 1024);

    for ((mode, _), (content, _)) in &expected_pairs {
        gateway
            .ranking_page(
                &context(),
                PixivRankingRequest {
                    mode: *mode,
                    content: *content,
                    date: Some(Date::from_calendar_date(2026, Month::July, 29).unwrap()),
                    page: 1,
                },
            )
            .await
            .unwrap();
    }

    let requests = server.finish();
    for (request, ((_, expected_mode), (_, expected_content))) in
        requests.iter().zip(expected_pairs)
    {
        let url = Url::parse(&format!("http://fixture{}", request.target)).unwrap();
        let query = url.query_pairs().collect::<BTreeMap<_, _>>();
        assert_eq!(
            query.get("format").map(|value| value.as_ref()),
            Some("json")
        );
        assert_eq!(
            query.get("mode").map(|value| value.as_ref()),
            Some(expected_mode)
        );
        assert_eq!(
            query.get("content").map(|value| value.as_ref()),
            Some(expected_content)
        );
        assert_eq!(
            query.get("date").map(|value| value.as_ref()),
            Some("20260729")
        );
        assert_eq!(query.get("p").map(|value| value.as_ref()), Some("1"));
    }
}

#[tokio::test]
async fn unsupported_ranking_combinations_are_rejected_before_network_access() {
    let server = TestServer::spawn(Vec::new());
    let gateway = client(&server, 1024, 1024);

    for (mode, content) in [
        (PixivRankingMode::Male, PixivRankingContent::Illustration),
        (PixivRankingMode::Female, PixivRankingContent::Manga),
        (PixivRankingMode::AiGenerated, PixivRankingContent::Ugoira),
        (PixivRankingMode::Original, PixivRankingContent::Manga),
        (PixivRankingMode::Monthly, PixivRankingContent::Ugoira),
        (PixivRankingMode::R18g, PixivRankingContent::Ugoira),
    ] {
        let error = gateway
            .ranking_page(
                &context(),
                PixivRankingRequest {
                    mode,
                    content,
                    date: None,
                    page: 1,
                },
            )
            .await
            .unwrap_err();
        assert_eq!(error.class(), PixivErrorClass::HiddenOrNotFound);
    }

    assert!(server.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn ranking_request_uses_typed_query_and_response_cursor() {
    let raw = fixture("ranking.json");
    let server = TestServer::spawn(vec![response(200, "application/json", raw.to_string())]);
    let gateway = client(&server, 64 * 1024, 64 * 1024);
    let request = PixivRankingRequest {
        mode: PixivRankingMode::Daily,
        content: PixivRankingContent::All,
        date: Some(Date::from_calendar_date(2026, Month::July, 29).unwrap()),
        page: 1,
    };

    let page = gateway
        .ranking_page(&context(), request)
        .await
        .unwrap()
        .value;
    assert_eq!(page.next_cursor.unwrap().page, 2);

    let requests = server.finish();
    let parsed = Url::parse(&format!("http://fixture{}", requests[0].target)).unwrap();
    let query = parsed.query_pairs().collect::<BTreeMap<_, _>>();
    assert_eq!(parsed.path(), "/ranking.php");
    assert_eq!(query.get("mode").map(|value| value.as_ref()), Some("daily"));
    assert_eq!(
        query.get("content").map(|value| value.as_ref()),
        Some("all")
    );
    assert_eq!(
        query.get("date").map(|value| value.as_ref()),
        Some("20260729")
    );
    assert_eq!(query.get("p").map(|value| value.as_ref()), Some("1"));
    assert_eq!(
        query.get("format").map(|value| value.as_ref()),
        Some("json")
    );
}

#[tokio::test]
async fn metadata_limits_cover_content_length_and_chunked_bodies() {
    let oversized = json!({"error": false, "body": {"padding": "x".repeat(128)}}).to_string();
    let length_server = TestServer::spawn(vec![response(
        200,
        "application/json",
        oversized.as_bytes(),
    )]);
    let length_client = client(&length_server, 32, 1024);
    let length_error = length_client
        .work_detail(&context(), 1001)
        .await
        .unwrap_err();
    assert_eq!(length_error.class(), PixivErrorClass::ResponseTooLarge);
    length_server.finish();

    let chunked_server = TestServer::spawn(vec![chunked_response(
        200,
        "application/json",
        oversized.as_bytes(),
    )]);
    let chunked_client = client(&chunked_server, 32, 1024);
    let chunked_error = chunked_client
        .work_detail(&context(), 1001)
        .await
        .unwrap_err();
    assert_eq!(chunked_error.class(), PixivErrorClass::ResponseTooLarge);
    chunked_server.finish();

    let error_length_server = TestServer::spawn(vec![response(
        502,
        "application/json",
        oversized.as_bytes(),
    )]);
    let error_length_client = client(&error_length_server, 32, 1024);
    let error_length = error_length_client
        .work_detail(&context(), 1001)
        .await
        .unwrap_err();
    assert_eq!(error_length.class(), PixivErrorClass::ResponseTooLarge);
    error_length_server.finish();

    let error_chunked_server = TestServer::spawn(vec![chunked_response(
        502,
        "application/json",
        oversized.as_bytes(),
    )]);
    let error_chunked_client = client(&error_chunked_server, 32, 1024);
    let error_chunked = error_chunked_client
        .work_detail(&context(), 1001)
        .await
        .unwrap_err();
    assert_eq!(error_chunked.class(), PixivErrorClass::ResponseTooLarge);
    error_chunked_server.finish();
}

#[tokio::test]
async fn invalid_collection_parameters_fail_before_network_access() {
    let ranking_server = TestServer::spawn(Vec::new());
    let ranking_client = client(&ranking_server, 1024, 1024);
    let ranking_error = ranking_client
        .ranking_page(
            &context(),
            PixivRankingRequest {
                mode: PixivRankingMode::Daily,
                content: PixivRankingContent::All,
                date: None,
                page: 0,
            },
        )
        .await
        .unwrap_err();
    assert_eq!(ranking_error.class(), PixivErrorClass::HiddenOrNotFound);
    assert!(ranking_server.finish().is_empty());

    let follow_server = TestServer::spawn(Vec::new());
    let follow_client = client(&follow_server, 1024, 1024);
    let follow_error = follow_client
        .follow_latest(
            &context(),
            PixivFollowLatestRequest {
                source: PixivFollowLatestSource::Following,
                mode: PixivFollowLatestMode::All,
                tag: None,
                language: "zh".to_owned(),
                page: 0,
            },
        )
        .await
        .unwrap_err();
    assert_eq!(follow_error.class(), PixivErrorClass::HiddenOrNotFound);
    assert!(follow_server.finish().is_empty());

    let bookmarks_server = TestServer::spawn(Vec::new());
    let bookmarks_client = client(&bookmarks_server, 1024, 1024);
    let bookmarks_error = bookmarks_client
        .bookmarks(
            &context(),
            PixivBookmarksRequest {
                user_id: 0,
                visibility: PixivBookmarkVisibility::Public,
                mode: PixivBookmarksMode::All,
                tag: None,
                offset: 0,
            },
        )
        .await
        .unwrap_err();
    assert_eq!(bookmarks_error.class(), PixivErrorClass::HiddenOrNotFound);
    assert!(bookmarks_server.finish().is_empty());

    let following_server = TestServer::spawn(Vec::new());
    let following_client = client(&following_server, 1024, 1024);
    let following_error = following_client
        .following_page(
            &context(),
            PixivFollowingRequest {
                user_id: 10001,
                visibility: PixivFollowingVisibility::Public,
                offset: 0,
                limit: 0,
                language: "zh".to_owned(),
            },
        )
        .await
        .unwrap_err();
    assert_eq!(following_error.class(), PixivErrorClass::HiddenOrNotFound);
    assert!(following_server.finish().is_empty());

    for request in [
        PixivFollowingRequest {
            user_id: 0,
            visibility: PixivFollowingVisibility::Public,
            offset: 0,
            limit: 1,
            language: "zh".to_owned(),
        },
        PixivFollowingRequest {
            user_id: 10001,
            visibility: PixivFollowingVisibility::Public,
            offset: 0,
            limit: 101,
            language: "zh".to_owned(),
        },
        PixivFollowingRequest {
            user_id: 10001,
            visibility: PixivFollowingVisibility::Public,
            offset: u32::MAX,
            limit: 1,
            language: "zh".to_owned(),
        },
    ] {
        let following_server = TestServer::spawn(Vec::new());
        let following_client = client(&following_server, 1024, 1024);
        let error = following_client
            .following_page(&context(), request)
            .await
            .unwrap_err();
        assert_eq!(error.class(), PixivErrorClass::HiddenOrNotFound);
        assert!(following_server.finish().is_empty());
    }
}

#[tokio::test]
async fn bookmarks_use_pixivs_48_item_ajax_page_shape() {
    let server = TestServer::spawn(vec![response(
        200,
        "application/json",
        fixture("bookmarks.json").to_string(),
    )]);
    let gateway = client(&server, 64 * 1024, 64 * 1024);

    gateway
        .bookmarks(
            &context(),
            PixivBookmarksRequest {
                user_id: 10001,
                visibility: PixivBookmarkVisibility::Public,
                mode: PixivBookmarksMode::All,
                tag: None,
                offset: 48,
            },
        )
        .await
        .unwrap();

    let requests = server.finish();
    let parsed = Url::parse(&format!("http://fixture{}", requests[0].target)).unwrap();
    let query = parsed.query_pairs().collect::<BTreeMap<_, _>>();
    assert_eq!(query.get("offset").map(|value| value.as_ref()), Some("48"));
    assert_eq!(query.get("limit").map(|value| value.as_ref()), Some("48"));
    assert_eq!(query.get("rest").map(|value| value.as_ref()), Some("show"));
    assert_eq!(query.get("lang").map(|value| value.as_ref()), Some("zh"));
}

#[tokio::test]
async fn following_page_uses_visibility_query_and_preserves_raw_json() {
    let public_raw = json!({
        "error": false,
        "message": "",
        "body": {
            "users": [
                {
                    "userId": "501",
                    "userName": "Test Artist",
                    "profileImageUrl": "https://i.pximg.net/user-profile/img/501.png"
                }
            ],
            "total": 2
        }
    });
    let private_raw = json!({
        "error": false,
        "message": "",
        "body": {
            "users": [
                {
                    "userId": 502,
                    "userName": "Private Artist"
                }
            ],
            "total": 1
        }
    });
    let server = TestServer::spawn(vec![
        response(200, "application/json", public_raw.to_string()),
        response(200, "application/json", private_raw.to_string()),
    ]);
    let gateway = client(&server, 64 * 1024, 64 * 1024);

    let public = gateway
        .following_page(
            &context(),
            PixivFollowingRequest {
                user_id: 10001,
                visibility: PixivFollowingVisibility::Public,
                offset: 0,
                limit: 1,
                language: "zh".to_owned(),
            },
        )
        .await
        .unwrap();
    let private = gateway
        .following_page(
            &context(),
            PixivFollowingRequest {
                user_id: 10001,
                visibility: PixivFollowingVisibility::Private,
                offset: 10,
                limit: 100,
                language: "zh".to_owned(),
            },
        )
        .await
        .unwrap();

    assert_eq!(public.value.items[0].pixiv_id, 501);
    assert_eq!(public.value.next_cursor.unwrap().offset, 1);
    assert_eq!(public.provenance.len(), 1);
    assert_eq!(public.provenance[0].endpoint, PixivEndpoint::Following);
    assert_eq!(public.provenance[0].raw, public_raw);
    assert_eq!(private.value.items[0].pixiv_id, 502);
    assert!(private.value.next_cursor.is_none());

    let requests = server.finish();
    assert_eq!(requests.len(), 2);
    for (request, rest, offset, limit) in [
        (&requests[0], "show", "0", "1"),
        (&requests[1], "hide", "10", "100"),
    ] {
        assert_eq!(request.method, "GET");
        let parsed = Url::parse(&format!("http://fixture{}", request.target)).unwrap();
        assert_eq!(parsed.path(), "/ajax/user/10001/following");
        let query = parsed.query_pairs().collect::<BTreeMap<_, _>>();
        assert_eq!(
            query.get("offset").map(|value| value.as_ref()),
            Some(offset)
        );
        assert_eq!(query.get("limit").map(|value| value.as_ref()), Some(limit));
        assert_eq!(query.get("rest").map(|value| value.as_ref()), Some(rest));
        assert_eq!(query.get("lang").map(|value| value.as_ref()), Some("zh"));
    }
}

#[tokio::test]
async fn redirects_and_html_are_not_followed_or_logged_as_json() {
    let redirect_server = TestServer::spawn(vec![response_with_headers(
        302,
        &[("Location", "/login"), ("Content-Type", "text/html")],
        "",
    )]);
    let redirect_client = client(&redirect_server, 1024, 1024);
    let redirect_error = redirect_client
        .work_detail(&context(), 1001)
        .await
        .unwrap_err();
    assert_eq!(
        redirect_error.class(),
        PixivErrorClass::InvalidJsonOrInterstitial
    );
    assert_eq!(redirect_server.finish().len(), 1);

    let html_server = TestServer::spawn(vec![response(
        200,
        "text/html",
        "<html>login interstitial</html>",
    )]);
    let html_client = client(&html_server, 1024, 1024);
    let html_error = html_client.work_detail(&context(), 1001).await.unwrap_err();
    assert_eq!(
        html_error.class(),
        PixivErrorClass::InvalidJsonOrInterstitial
    );
    assert!(!html_error.to_string().contains("login interstitial"));
    html_server.finish();
}

#[tokio::test]
async fn response_errors_do_not_retain_echoed_request_secrets() {
    let cookie_echo = json!({
        "error": true,
        "message": "session PHPSESSID=http-contract-cookie expired",
        "body": null
    })
    .to_string();
    let server = TestServer::spawn(vec![response(401, "application/json", cookie_echo)]);
    let gateway = client(&server, 64 * 1024, 64 * 1024);

    let error = gateway.work_detail(&context(), 1001).await.unwrap_err();

    assert_eq!(error.class(), PixivErrorClass::CredentialInvalid);
    assert!(!error.to_string().contains("http-contract-cookie"));
    assert!(!format!("{error:?}").contains("http-contract-cookie"));
    server.finish();
}

#[tokio::test]
async fn account_validation_requires_private_own_account_evidence() {
    let identity = json!({
        "error": false,
        "message": "",
        "body": {
            "userId": "10001",
            "name": "Test Artist"
        }
    })
    .to_string();
    let profile = fixture("profile_all.json").to_string();
    let public_shape = json!({
        "error": false,
        "message": "",
        "body": {"works": [], "total": 0}
    })
    .to_string();
    let rejected_server = TestServer::spawn(vec![
        response(200, "application/json", &identity),
        response(200, "application/json", &profile),
        response(200, "application/json", &public_shape),
    ]);
    let rejected_client = client(&rejected_server, 64 * 1024, 64 * 1024);
    let error = rejected_client
        .validate_account(&context())
        .await
        .unwrap_err();
    assert_eq!(error.class(), PixivErrorClass::CredentialInvalid);
    let rejected_requests = rejected_server.finish();
    assert_eq!(rejected_requests.len(), 3);
    assert_eq!(rejected_requests[0].target, "/ajax/user/10001?full=1");
    assert!(rejected_requests[2].target.contains("rest=hide"));

    let private_shape = fixture("bookmarks.json").to_string();
    let accepted_server = TestServer::spawn(vec![
        response(200, "application/json", &identity),
        response(200, "application/json", &profile),
        response(200, "application/json", &private_shape),
    ]);
    let accepted_client = client(&accepted_server, 64 * 1024, 64 * 1024);
    let validation = accepted_client.validate_account(&context()).await.unwrap();
    assert_eq!(validation.value.user_id, 10001);
    assert_eq!(validation.value.display_name, "Test Artist");
    assert!(validation.value.private_bookmarks_verified);
    assert_eq!(validation.provenance.len(), 3);
    accepted_server.finish();
}

#[tokio::test]
async fn csrf_error_responses_obey_the_html_response_limit() {
    let server = TestServer::spawn(vec![response(502, "text/html", "x".repeat(128).as_bytes())]);
    let gateway = client(&server, 64 * 1024, 32);

    let error = gateway
        .add_bookmark(
            &context(),
            PixivBookmarkAddRequest {
                work_id: 1001,
                visibility: PixivBookmarkVisibility::Public,
                tags: Vec::new(),
            },
        )
        .await
        .unwrap_err();

    assert_eq!(error.class(), PixivErrorClass::ResponseTooLarge);
    assert_eq!(server.finish().len(), 1);
}

#[tokio::test]
async fn bookmark_write_refreshes_csrf_once_then_returns_the_second_failure() {
    let first_failure = json!({
        "error": true,
        "message": "Invalid CSRF token csrf-canary-one",
        "body": null
    })
    .to_string();
    let second_failure = json!({
        "error": true,
        "message": "Invalid CSRF token csrf-canary-two",
        "body": null
    })
    .to_string();
    let server = TestServer::spawn(vec![
        response(
            200,
            "text/html",
            r#"<script id="__NEXT_DATA__">{"token":"csrf-canary-one"}</script>"#,
        ),
        response(400, "application/json", &first_failure),
        response(
            200,
            "text/html",
            r#"<script id="__NEXT_DATA__">{"token":"csrf-canary-two"}</script>"#,
        ),
        response(400, "application/json", &second_failure),
    ]);
    let gateway = client(&server, 64 * 1024, 64 * 1024);
    let error = gateway
        .add_bookmark(
            &context(),
            PixivBookmarkAddRequest {
                work_id: 1001,
                visibility: PixivBookmarkVisibility::Public,
                tags: vec!["blue".to_owned()],
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.class(), PixivErrorClass::CsrfFailed);

    let requests = server.finish();
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[0].target, "/");
    assert_eq!(requests[1].target, "/ajax/illusts/bookmarks/add");
    assert_eq!(requests[2].target, "/");
    assert_eq!(requests[3].target, "/ajax/illusts/bookmarks/add");
    assert_eq!(
        requests[1].headers.get("x-csrf-token").map(String::as_str),
        Some("csrf-canary-one")
    );
    assert_eq!(
        requests[3].headers.get("x-csrf-token").map(String::as_str),
        Some("csrf-canary-two")
    );
    assert!(!format!("{error:?}").contains("csrf-canary"));
    assert!(!error.to_string().contains("csrf-canary"));
}

#[tokio::test]
async fn delete_bookmark_sends_the_positive_bookmark_id_as_form_data() {
    let success = json!({"error": false, "message": "", "body": {}}).to_string();
    let server = TestServer::spawn(vec![
        response(
            200,
            "text/html",
            r#"<script id="__NEXT_DATA__">{"token":"delete-token"}</script>"#,
        ),
        response(200, "application/json", success),
    ]);
    let gateway = client(&server, 64 * 1024, 64 * 1024);

    gateway.delete_bookmark(&context(), 9001).await.unwrap();

    let requests = server.finish();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].method, "POST");
    assert_eq!(requests[1].target, "/ajax/illusts/bookmarks/delete");
    assert_eq!(requests[1].body, "bookmark_id=9001");
    assert_eq!(
        requests[1].headers.get("content-type").map(String::as_str),
        Some("application/x-www-form-urlencoded; charset=utf-8")
    );
}

#[tokio::test]
async fn artist_follow_state_uses_the_authenticated_profile_field() {
    let profile = json!({
        "error": false,
        "message": "",
        "body": {
            "userId": "70001",
            "name": "Artist Alpha",
            "image": "https://i.pximg.net/user-profile/img/70001.jpg",
            "isFollowed": true
        }
    })
    .to_string();
    let server = TestServer::spawn(vec![response(200, "application/json", profile)]);
    let gateway = client(&server, 64 * 1024, 64 * 1024);

    let state = gateway
        .artist_follow_state(&context(), 70001)
        .await
        .unwrap()
        .value;

    assert_eq!(state.artist_id, 70001);
    assert_eq!(state.name, "Artist Alpha");
    assert!(state.followed);
    let requests = server.finish();
    assert_eq!(requests[0].target, "/ajax/user/70001?full=1");
}

#[tokio::test]
async fn artist_follow_writes_match_the_current_pixiv_web_contract() {
    let server = TestServer::spawn(vec![
        response(
            200,
            "text/html",
            r#"<script id="__NEXT_DATA__">{"token":"artist-follow-token"}</script>"#,
        ),
        response(200, "application/json", "[]"),
        response(200, "application/json", r#"{"user_id":"70001"}"#),
    ]);
    let gateway = client(&server, 64 * 1024, 64 * 1024);

    gateway
        .add_artist_follow(
            &context(),
            PixivArtistFollowRequest {
                artist_id: 70001,
                visibility: PixivFollowingVisibility::Public,
            },
        )
        .await
        .unwrap();
    gateway
        .remove_artist_follow(&context(), 70001)
        .await
        .unwrap();

    let requests = server.finish();
    assert_eq!(requests[1].target, "/bookmark_add.php");
    assert_eq!(
        requests[1].body,
        "mode=add&type=user&user_id=70001&tag=&restrict=0&format=json"
    );
    assert_eq!(requests[2].target, "/rpc_group_setting.php");
    assert_eq!(requests[2].body, "mode=del&type=bookuser&id=70001");
    for request in &requests[1..] {
        assert_eq!(request.method, "POST");
        assert_eq!(
            request.headers.get("x-csrf-token").map(String::as_str),
            Some("artist-follow-token")
        );
    }
}

#[tokio::test]
async fn media_requests_allow_exact_hosts_and_use_the_artwork_referer() {
    let server = TestServer::spawn(vec![response(200, "image/jpeg", b"source-bytes")]);
    let gateway = client(&server, 64 * 1024, 64 * 1024);
    let media_url = server.base_url.join("source.jpg").unwrap();

    let media_gateway: &dyn PixivMediaGateway = &gateway;
    let response = media_gateway
        .media(&context(), 1001, media_url)
        .await
        .unwrap();
    assert_eq!(response.content_length, Some(12));
    assert_eq!(response.content_type.as_deref(), Some("image/jpeg"));
    let chunks = response.body.try_collect::<Vec<_>>().await.unwrap();
    assert_eq!(chunks.concat(), b"source-bytes");

    let requests = server.finish();
    assert_eq!(
        requests[0].headers.get("referer").map(String::as_str),
        Some("https://www.pixiv.net/artworks/1001")
    );
    assert_eq!(
        requests[0]
            .headers
            .get("accept-encoding")
            .map(String::as_str),
        Some("identity")
    );

    let rejected = media_gateway
        .media(
            &context(),
            1001,
            Url::parse("https://evil-i.pximg.net/source.jpg").unwrap(),
        )
        .await
        .unwrap_err();
    assert_eq!(rejected.class(), PixivErrorClass::RefererForbidden);

    let default_gateway = PixivWebClient::new(PixivClientOptions::default()).unwrap();
    let default_media_gateway: &dyn PixivMediaGateway = &default_gateway;
    let insecure = default_media_gateway
        .media(
            &context(),
            1001,
            Url::parse("http://i.pximg.net/source.jpg").unwrap(),
        )
        .await
        .unwrap_err();
    assert_eq!(insecure.class(), PixivErrorClass::RefererForbidden);
}

#[test]
fn official_pixiv_asset_urls_cover_profile_and_default_avatars() {
    assert!(is_official_pixiv_asset_url(
        &Url::parse("https://i.pximg.net/user-profile/img/avatar.jpg").unwrap()
    ));
    assert!(is_official_pixiv_asset_url(
        &Url::parse("https://s.pximg.net/common/images/no_profile.png").unwrap()
    ));
    assert!(!is_official_pixiv_asset_url(
        &Url::parse("https://evil-i.pximg.net/avatar.jpg").unwrap()
    ));
    assert!(!is_official_pixiv_asset_url(
        &Url::parse("http://s.pximg.net/common/images/no_profile.png").unwrap()
    ));
}

#[tokio::test]
async fn media_concurrency_permit_lives_until_the_response_stream_is_dropped() {
    let server = TestServer::spawn(vec![
        response(200, "image/jpeg", b"first"),
        response(200, "image/jpeg", b"second"),
    ]);
    let gateway = PixivWebClient::new(PixivClientOptions {
        web_base_url: server.base_url.clone(),
        allowed_media_hosts: BTreeSet::from([server.base_url.host_str().unwrap().to_owned()]),
        metadata_response_limit: 64 * 1024,
        csrf_response_limit: 64 * 1024,
        request_timeout: StdDuration::from_secs(2),
        use_system_proxy: false,
        metadata_request_gate: None,
        media_request_gate: Some(PixivRequestGate::new(1, None).unwrap()),
    })
    .unwrap();
    let first = gateway
        .media(&context(), 1001, server.base_url.join("first.jpg").unwrap())
        .await
        .unwrap();

    assert!(
        tokio::time::timeout(
            StdDuration::from_millis(40),
            gateway.media(
                &context(),
                1001,
                server.base_url.join("second.jpg").unwrap()
            )
        )
        .await
        .is_err()
    );

    drop(first);
    let second = gateway
        .media(
            &context(),
            1001,
            server.base_url.join("second.jpg").unwrap(),
        )
        .await
        .unwrap();
    let chunks = second.body.try_collect::<Vec<_>>().await.unwrap();
    assert_eq!(chunks.concat(), b"second");
    assert_eq!(server.finish().len(), 2);
}
