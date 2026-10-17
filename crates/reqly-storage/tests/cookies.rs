use reqly_core::{FixedClock, Id};
use reqly_storage::Storage;
use reqly_storage::cookies::{Cookie, CookieRepo};

#[test]
fn parses_and_formats_netscape_lines() {
    let line = "#HttpOnly_.example.com\tTRUE\t/api\tTRUE\t1893456000\tsid\tabc";
    let c = Cookie::from_netscape(line).unwrap();
    assert_eq!(c.domain, "example.com");
    assert!(c.include_subdomains && c.http_only && c.secure);
    assert_eq!(c.path, "/api");
    assert_eq!(c.expires, Some(1_893_456_000));
    assert_eq!((c.name.as_str(), c.value.as_str()), ("sid", "abc"));
    assert_eq!(c.to_netscape(), line);

    let session = Cookie::from_netscape("127.0.0.1\tFALSE\t/\tFALSE\t0\ta\tb").unwrap();
    assert_eq!(session.expires, None);
    assert_eq!(session.to_netscape(), "127.0.0.1\tFALSE\t/\tFALSE\t0\ta\tb");
    assert!(Cookie::from_netscape("garbage").is_none());
}

#[tokio::test]
async fn replace_list_upsert_delete() {
    let s = Storage::in_memory().await.unwrap();
    let clock = FixedClock::new(1);
    let repo = CookieRepo::new(&s, &clock);
    let ws = Id::new();
    repo.replace_all(
        ws,
        &[
            "a.com\tFALSE\t/\tFALSE\t0\tx\t1".into(),
            "b.com\tFALSE\t/\tFALSE\t0\ty\t2".into(),
        ],
    )
    .await
    .unwrap();
    let list = repo.list(ws).await.unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(repo.netscape_lines(ws).await.unwrap().len(), 2);

    let mut c = list[0].clone();
    c.value = "changed".into();
    repo.upsert(ws, &c).await.unwrap();
    assert_eq!(
        repo.list(ws)
            .await
            .unwrap()
            .iter()
            .find(|x| x.name == c.name)
            .unwrap()
            .value,
        "changed"
    );

    repo.delete(ws, &[c.id]).await.unwrap();
    assert_eq!(repo.list(ws).await.unwrap().len(), 1);
    repo.clear(ws).await.unwrap();
    assert!(repo.list(ws).await.unwrap().is_empty());
    assert!(repo.list(Id::new()).await.unwrap().is_empty());
}

#[tokio::test]
async fn expired_cookies_are_not_listed() {
    let s = Storage::in_memory().await.unwrap();
    let clock = FixedClock::new(2_000_000);
    let repo = CookieRepo::new(&s, &clock);
    let ws = Id::new();
    repo.replace_all(ws, &["a.com\tFALSE\t/\tFALSE\t1000\told\t1".into()])
        .await
        .unwrap();
    assert!(repo.list(ws).await.unwrap().is_empty());
}
