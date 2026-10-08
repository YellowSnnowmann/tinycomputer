//! Tests for which web page a task's own words send its browser to.

use super::super::page::named_page;

#[test]
fn a_task_names_the_one_address_it_sends_its_browser_to() {
    assert_eq!(
        named_page("Go to https://blinkit.com. Search for Maggi.").as_deref(),
        Some("https://blinkit.com")
    );
    // As OpenHuman writes the address a task starts at.
    assert_eq!(
        named_page("Order Maggi. Start at https://www.amazon.in.").as_deref(),
        Some("https://www.amazon.in")
    );
    // Kept as written, a path too, without what closes the sentence.
    assert_eq!(
        named_page("Book a cab on https://www.uber.com/global/en/price-estimate/, then stop.")
            .as_deref(),
        Some("https://www.uber.com/global/en/price-estimate/")
    );
    assert_eq!(
        named_page("Visit: (http://shop.test/deals)").as_deref(),
        Some("http://shop.test/deals")
    );
    // Written twice, with a trailing slash or without, it is one page.
    assert_eq!(
        named_page("Open https://mail.test and stay on https://mail.test/").as_deref(),
        Some("https://mail.test")
    );
}

#[test]
fn an_address_only_mentioned_or_one_a_load_could_spend_is_not_loaded() {
    // Checked, passed on, or read, not gone to.
    assert_eq!(
        named_page("Check https://paypa1-secure.test/login on VirusTotal"),
        None
    );
    assert_eq!(named_page("Post https://shop.test in the team chat"), None);
    // A query or a fragment can hold a token.
    assert_eq!(
        named_page("Go to https://acct.test/confirm?token=f00d to finish"),
        None
    );
    assert_eq!(named_page("Open https://app.test/#/cart"), None);
    // "on" and "at" only as words of their own.
    assert_eq!(named_page("Reason https://shop.test"), None);
    assert_eq!(
        named_page("Summarize the page at https://news.test?").as_deref(),
        Some("https://news.test"),
        "read there, so gone to"
    );
}

#[test]
fn a_task_naming_no_page_or_several_names_none() {
    assert_eq!(named_page("Order Maggi on Blinkit"), None);
    assert_eq!(
        named_page("Compare https://a.test with https://b.test"),
        None
    );
    assert_eq!(
        named_page("Go to https://shop.test and paste https://acct.test/confirm"),
        None,
        "another address, mentioned, leaves no one page"
    );
    assert_eq!(named_page("Type https:// into the box"), None);
    assert_eq!(named_page(""), None);
}
