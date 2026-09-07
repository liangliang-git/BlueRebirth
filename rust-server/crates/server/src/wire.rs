pub(crate) fn append_varint_field(output: &mut Vec<u8>, field: u8, value: u64) {
    append_varint(output, u64::from(field) << 3);
    append_varint(output, value);
}

pub(crate) fn append_varint(output: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        output.push((value as u8) | 0x80);
        value >>= 7;
    }
    output.push(value as u8);
}

pub(crate) fn append_message_field(output: &mut Vec<u8>, field: u8, message: &[u8]) {
    append_varint(output, (u64::from(field) << 3) | 2);
    append_varint(output, message.len() as u64);
    output.extend_from_slice(message);
}

pub(crate) fn append_bytes_field(output: &mut Vec<u8>, field: u8, value: &[u8]) {
    append_message_field(output, field, value);
}

pub(crate) fn read_varint(payload: &[u8], mut index: usize) -> Result<(u64, usize), ()> {
    let mut value = 0_u64;
    for shift in (0..70).step_by(7) {
        let byte = *payload.get(index).ok_or(())?;
        index += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok((value, index));
        }
    }
    Err(())
}

#[cfg(test)]
pub(crate) fn read_string_slice(payload: &[u8], index: usize) -> Option<(&[u8], usize)> {
    let (length, next) = read_varint(payload, index).ok()?;
    let length = usize::try_from(length).ok()?;
    let end = next.checked_add(length)?;
    Some((payload.get(next..end)?, end))
}

#[cfg(test)]
pub(crate) fn skip_wire(payload: &[u8], index: usize, wire: u64) -> Option<usize> {
    match wire {
        0 => read_varint(payload, index).ok().map(|(_, next)| next),
        1 => index.checked_add(8).filter(|end| *end <= payload.len()),
        2 => read_string_slice(payload, index).map(|(_, next)| next),
        5 => index.checked_add(4).filter(|end| *end <= payload.len()),
        _ => None,
    }
}
