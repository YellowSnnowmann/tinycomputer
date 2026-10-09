//! Tests for which pages a session's allowed origins admit.

use super::Origins;

fn origins(list: &[&str]) -> Origins {
    Origins::new(
        &list
            .iter()
            .map(|origin| (*origin).to_owned())
            .collect::<Vec<_>>(),
    )
}

#[test]
fn no_list_admits_every_page() {
    let any = origins(&[]);
    for url in [
        "https://example.com/",
        "http://localhost:8080/",
        "file:///etc/hosts",
        "data:text/html,hi",
    ] {
        assert!(any.admits(url), "{url}");
    }
}

#[test]
fn a_host_admits_itself_alone_whatever_the_scheme_port_or_case() {
    let list = origins(&["https://Example.com"]);
    for url in [
        "https://example.com/flights?from=DEL",
        "http://EXAMPLE.com:8443/",
        "https://user:secret@example.com/",
        "example.com/path",
        "https://example.com./",
    ] {
        assert!(list.admits(url), "{url}");
    }
    for url in [
        "https://www.example.com/",
        "https://example.com.evil.test/",
        "https://notexample.com/",
        "https://evil.test/?next=example.com",
        "https://evil.test/#example.com",
    ] {
        assert!(!list.admits(url), "{url}");
    }
}

#[test]
fn a_dotted_host_admits_itself_and_every_subdomain() {
    for spelling in [".agoda.com", "https://.agoda.com", "*.agoda.com"] {
        let list = origins(&[spelling]);
        for url in [
            "https://agoda.com/",
            "https://www.agoda.com/search",
            "https://secure.book.agoda.com/",
        ] {
            assert!(list.admits(url), "{spelling} {url}");
        }
        for url in [
            "https://notagoda.com/",
            "https://agoda.com.evil.test/",
            "https://agoda.net/",
        ] {
            assert!(!list.admits(url), "{spelling} {url}");
        }
    }
}

#[test]
fn a_star_admits_any_public_host_but_never_a_local_or_private_one() {
    let list = origins(&["*"]);
    for url in [
        "https://www.makemytrip.com/flights/",
        "https://93.184.216.34/",
        "https://[2606:4700::1111]/",
    ] {
        assert!(list.admits(url), "{url}");
    }
    for url in [
        "http://localhost:3000/",
        "http://printer.local/",
        "http://app.localhost/",
        "http://127.0.0.1/",
        "http://10.1.2.3/",
        "http://172.16.0.9/",
        "http://192.168.1.1/admin",
        "http://169.254.169.254/latest/meta-data",
        "http://100.64.0.1/",
        "http://0.0.0.0/",
        "http://[::1]/",
        "http://[fd00::5]/",
        "http://[fe80::1]/",
        "http://[::ffff:192.168.0.1]/",
    ] {
        assert!(!list.admits(url), "{url}");
    }
}

#[test]
fn a_page_that_is_no_site_shows_and_other_schemes_never_do_under_a_list() {
    let list = origins(&["https://.example.com"]);
    assert!(list.admits("about:blank"));
    assert!(list.admits("chrome-error://chromewebdata/"));
    for url in [
        "file:///Users/someone/.ssh/id_rsa",
        "data:text/html,<script>1</script>",
        "javascript:alert(1)",
        "chrome://settings",
        "view-source:https://example.com/",
        "ftp://example.com/",
    ] {
        assert!(!list.admits(url), "{url}");
    }
}

#[test]
fn a_list_whose_entries_name_no_host_admits_nothing() {
    let list = origins(&["https://", "   ", "."]);
    assert!(!list.admits("https://example.com/"));
    assert!(list.admits("about:blank"));
}
