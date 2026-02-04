/// Returns the next pooling frequency based on the lastest pending deposit timestamp.
pub fn pooling_freq(
    last_pending: time::PrimitiveDateTime,
    now: time::PrimitiveDateTime,
) -> time::Duration {
    let last_waited = now - last_pending;
    match last_waited {
        d if d < time::Duration::seconds(5) => time::Duration::seconds(2),
        d if d < time::Duration::seconds(10) => time::Duration::seconds(10),
        d if d < time::Duration::seconds(30) => time::Duration::seconds(30),
        _ => time::Duration::seconds(60),
    }
}
