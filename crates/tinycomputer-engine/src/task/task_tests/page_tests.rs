//! Tests for which web page a task's own words name.

use super::super::page::named_page;

#[test]
fn a_task_names_the_one_address_it_writes_out() {
    assert_eq!(
        named_page("Go to https://blinkit.com. Search for Maggi.").as_deref(),
        Some("https://blinkit.com")
    );
    // Kept as written, a path and query too, without what closes the sentence.
    assert_eq!(
        named_page("Book a cab on https://www.uber.com/global/en/price-estimate/, then stop.")
            .as_deref(),
        Some("https://www.uber.com/global/en/price-estimate/")
    );
    assert_eq!(
        named_page("Open [the shop](http://shop.test/?q=boots)").as_deref(),
        Some("http://shop.test/?q=boots")
    );
    // Written twice, with a trailing slash or without, it is one page.
    assert_eq!(
        named_page("Open https://mail.test and stay on https://mail.test/").as_deref(),
        Some("https://mail.test")
    );
}

#[test]
fn a_task_naming_no_page_or_several_names_none() {
    assert_eq!(named_page("Order Maggi on Blinkit"), None);
    assert_eq!(
        named_page("Compare https://a.test with https://b.test"),
        None
    );
    assert_eq!(named_page("Type https:// into the box"), None);
    assert_eq!(named_page(""), None);
}
