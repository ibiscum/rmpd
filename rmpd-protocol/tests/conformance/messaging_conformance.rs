//! Tests for MPD client-to-client messaging commands over TCP.

use crate::tcp_harness::*;

#[tokio::test]
async fn subscribe_and_unsubscribe() {
    let (_server, mut client) = setup().await;

    let resp = client.command("subscribe \"testchan\"").await;
    assert_ok(&resp);

    let resp = client.command("unsubscribe \"testchan\"").await;
    assert_ok(&resp);
}

#[tokio::test]
async fn subscribe_returns_ok() {
    let (_server, mut client) = setup().await;
    let resp = client.command("subscribe \"mychannel\"").await;
    assert_ok(&resp);
}

#[tokio::test]
async fn sendmessage_and_readmessages() {
    let server = MpdTestServer::start().await;
    let mut client1 = MpdTestClient::connect(server.port()).await;
    let mut client2 = MpdTestClient::connect(server.port()).await;

    let resp = client1.command("subscribe \"msgchan\"").await;
    assert_ok(&resp);

    let resp = client2.command("sendmessage \"msgchan\" \"hello\"").await;
    assert_ok(&resp);

    let resp = client1.command("readmessages").await;
    assert_eq!(resp, "channel: msgchan\nmessage: hello\nOK\n");

    let resp = client1.command("readmessages").await;
    assert_eq!(resp, "OK\n");
}

#[tokio::test]
async fn readmessages_empty() {
    let (_server, mut client) = setup().await;
    let resp = client.command("readmessages").await;
    assert_ok(&resp);
}

#[tokio::test]
async fn channels_returns_ok() {
    let server = MpdTestServer::start().await;
    let mut subscriber = MpdTestClient::connect(server.port()).await;
    let mut observer = MpdTestClient::connect(server.port()).await;

    let resp = subscriber.command("subscribe \"testchan\"").await;
    assert_ok(&resp);

    let resp = observer.command("channels").await;
    assert_eq!(resp, "channel: testchan\nOK\n");

    let resp = subscriber.command("unsubscribe \"testchan\"").await;
    assert_ok(&resp);

    let resp = observer.command("channels").await;
    assert_eq!(resp, "OK\n");
}

#[tokio::test]
async fn sendmessage_fanout_reaches_all_subscribers() {
    let server = MpdTestServer::start().await;
    let mut a = MpdTestClient::connect(server.port()).await;
    let mut b = MpdTestClient::connect(server.port()).await;
    let mut sender = MpdTestClient::connect(server.port()).await;

    assert_ok(&a.command("subscribe \"fanout\"").await);
    assert_ok(&b.command("subscribe \"fanout\"").await);

    let resp = sender.command("sendmessage \"fanout\" \"hello-all\"").await;
    assert_ok(&resp);

    let resp_a = a.command("readmessages").await;
    let resp_b = b.command("readmessages").await;

    assert_eq!(resp_a, "channel: fanout\nmessage: hello-all\nOK\n");
    assert_eq!(resp_b, "channel: fanout\nmessage: hello-all\nOK\n");
}
