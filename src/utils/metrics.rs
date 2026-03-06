use once_cell::sync::Lazy;
use prometheus::{IntCounter, IntGauge, Histogram, HistogramOpts, register_int_counter, register_int_gauge, register_histogram};

// Channel fullness metric
pub static SEND_FULL_COUNTER: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!("chat_send_full_total", "Number of failed try_send due to full channel").expect("register counter")
});

// Message counters
pub static MESSAGES_SENT_TOTAL: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!("chat_messages_sent_total", "Total messages sent (room + private)").expect("register counter")
});

pub static PRIVATE_MESSAGES_TOTAL: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!("chat_private_messages_total", "Total private messages sent").expect("register counter")
});

pub static ROOM_MESSAGES_TOTAL: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!("chat_room_messages_total", "Total room messages sent").expect("register counter")
});

pub static MESSAGES_DELIVERED_TOTAL: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!("chat_messages_delivered_total", "Messages delivered immediately (recipient online)").expect("register counter")
});

pub static MESSAGES_QUEUED_TOTAL: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!("chat_messages_queued_total", "Messages queued for offline delivery").expect("register counter")
});

pub static OFFLINE_MESSAGES_DELIVERED_TOTAL: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!("chat_offline_messages_delivered_total", "Offline messages delivered on login").expect("register counter")
});

pub static MESSAGE_QUEUE_FULL_TOTAL: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!("chat_message_queue_full_total", "Messages rejected due to full recipient queue").expect("register counter")
});

// User activity counters
pub static LOGINS_TOTAL: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!("chat_logins_total", "Total successful logins").expect("register counter")
});

pub static REGISTRATIONS_TOTAL: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!("chat_registrations_total", "Total user registrations").expect("register counter")
});

pub static LOGOUTS_TOTAL: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!("chat_logouts_total", "Total logouts").expect("register counter")
});

pub static ROOM_JOINS_TOTAL: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!("chat_room_joins_total", "Total room joins").expect("register counter")
});

pub static ROOM_LEAVES_TOTAL: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!("chat_room_leaves_total", "Total room leaves").expect("register counter")
});

// Gauges for current state
pub static USERS_ONLINE: Lazy<IntGauge> = Lazy::new(|| {
    register_int_gauge!("chat_users_online", "Current number of users online").expect("register gauge")
});

pub static PENDING_MESSAGES: Lazy<IntGauge> = Lazy::new(|| {
    register_int_gauge!("chat_pending_messages", "Current number of pending offline messages").expect("register gauge")
});

pub static ACTIVE_ROOMS: Lazy<IntGauge> = Lazy::new(|| {
    register_int_gauge!("chat_active_rooms", "Current number of active rooms").expect("register gauge")
});

// Histogram for latency
pub static MESSAGE_LATENCY: Lazy<Histogram> = Lazy::new(|| {
    let opts = HistogramOpts::new("chat_message_latency_seconds", "Message delivery latency")
        .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0]);
    register_histogram!(opts).expect("register histogram")
});

// Convenience functions
pub fn inc_send_full() {
    SEND_FULL_COUNTER.inc();
}

pub fn inc_messages_sent() {
    MESSAGES_SENT_TOTAL.inc();
}

pub fn inc_private_messages() {
    PRIVATE_MESSAGES_TOTAL.inc();
}

pub fn inc_room_messages() {
    ROOM_MESSAGES_TOTAL.inc();
}

pub fn inc_messages_delivered() {
    MESSAGES_DELIVERED_TOTAL.inc();
}

pub fn inc_messages_queued() {
    MESSAGES_QUEUED_TOTAL.inc();
}

pub fn inc_offline_messages_delivered() {
    OFFLINE_MESSAGES_DELIVERED_TOTAL.inc();
}

pub fn inc_queue_full() {
    MESSAGE_QUEUE_FULL_TOTAL.inc();
}

pub fn inc_logins() {
    LOGINS_TOTAL.inc();
}

pub fn inc_registrations() {
    REGISTRATIONS_TOTAL.inc();
}

pub fn inc_logouts() {
    LOGOUTS_TOTAL.inc();
}

pub fn inc_room_joins() {
    ROOM_JOINS_TOTAL.inc();
}

pub fn inc_room_leaves() {
    ROOM_LEAVES_TOTAL.inc();
}

pub fn set_users_online(count: i64) {
    USERS_ONLINE.set(count);
}

pub fn set_pending_messages(count: i64) {
    PENDING_MESSAGES.set(count);
}

pub fn set_active_rooms(count: i64) {
    ACTIVE_ROOMS.set(count);
}

pub fn observe_message_latency(duration: f64) {
    MESSAGE_LATENCY.observe(duration);
}
