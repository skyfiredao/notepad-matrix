use notepad_matrix::byte_buffer::{ByteBuffer, InvalidByteRange};

#[test]
fn raw_line_coordinates_keep_cross_leaf_crlf_and_snapshot_metrics()
-> Result<(), Box<dyn std::error::Error>> {
    let bytes = [vec![b'A'; 4095], b"\r\n".to_vec(), vec![b'B'; 4095]].concat();
    let crossing = ByteBuffer::from_bytes(&bytes);
    assert_eq!(crossing.line_count(), 2);
    assert_eq!(crossing.line_start(1)?, 4097);
    assert_eq!(crossing.line_index(4096)?, 0);
    assert_eq!(crossing.line_index(4097)?, 1);
    let split_pair = crossing.slice(4096..4097)?;
    assert_eq!(split_pair.line_count(), 2);
    assert_eq!(split_pair.line_start(1)?, 1);
    assert_eq!(split_pair.line_index(1)?, 1);
    let mut buffer = ByteBuffer::from_bytes(b"\xff\r\n\x80\rZ\n");
    let original = buffer.clone();
    assert_eq!(buffer.line_count(), 4);
    for (line, start) in [0, 3, 5, 7].into_iter().enumerate() {
        assert_eq!(buffer.line_start(line)?, start);
        assert_eq!(buffer.line_index(start)?, line);
    }
    assert_eq!(buffer.line_index(2)?, 0);
    buffer.replace(2..3, b"")?;
    assert_eq!(buffer.line_count(), 4);
    assert_eq!(buffer.line_start(1)?, 2);
    buffer.replace(1..2, b"")?;
    assert_eq!(buffer.line_count(), 3);
    assert_eq!(buffer.line_start(1)?, 3);
    assert_eq!(buffer.line_start(2)?, 5);
    assert_eq!(original.line_count(), 4);
    assert_eq!(original.line_start(1)?, 3);
    assert!(buffer.line_index(buffer.len() + 1).is_err());
    assert!(buffer.line_start(3).is_err());
    assert_eq!(ByteBuffer::default().line_count(), 1);
    assert_eq!(ByteBuffer::default().line_start(0)?, 0);
    Ok(())
}

#[test]
fn source_byte_units_keep_invalid_bytes_independent_across_leaf_boundaries() {
    use notepad_matrix::byte_text::ByteValue;
    let buffer = ByteBuffer::from_bytes(b"A\xed\xb0\x80\xf0\x9f\x98\x80\xc0\xaf\xef\xbf\xbe\r\n");
    let units = buffer.units().collect::<Vec<_>>();
    assert_eq!(
        units.iter().map(|unit| unit.value).collect::<Vec<_>>(),
        [
            ByteValue::Scalar('A'),
            ByteValue::Invalid(0xed),
            ByteValue::Invalid(0xb0),
            ByteValue::Invalid(0x80),
            ByteValue::Scalar('😀'),
            ByteValue::Invalid(0xc0),
            ByteValue::Invalid(0xaf),
            ByteValue::Invalid(0xef),
            ByteValue::Invalid(0xbf),
            ByteValue::Invalid(0xbe),
            ByteValue::Scalar('\r'),
            ByteValue::Scalar('\n'),
        ]
    );
    assert_eq!(units[4].range, 4..8);
    let bytes = [vec![b'A'; 4095], "😀".as_bytes().to_vec(), vec![b'B'; 4093]].concat();
    let crossing = ByteBuffer::from_bytes(&bytes);
    let unit = crossing.units().nth(4095).expect("cross-leaf scalar");
    assert_eq!(unit.value, ByteValue::Scalar('😀'));
    assert_eq!(unit.range, 4095..4099);
}

#[test]
fn byte_storage_matches_raw_splices_and_keeps_captured_versions()
-> Result<(), Box<dyn std::error::Error>> {
    let mut expected = (0..65_537)
        .map(|index| (index % 256) as u8)
        .collect::<Vec<_>>();
    let mut buffer = ByteBuffer::from_bytes(&expected);
    let mut snapshots = Vec::new();
    let mut state = 0x1234_5678u64;
    let mut random = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state as usize
    };
    for step in 0..5000 {
        let start = random() % (expected.len() + 1);
        let end = (start + random() % 30).min(expected.len());
        let replacement = (0..random() % 24)
            .map(|_| random() as u8)
            .collect::<Vec<_>>();
        if step % 127 == 0 {
            snapshots.push((buffer.clone(), expected.clone()));
        }
        buffer.replace(start..end, &replacement)?;
        expected.splice(start..end, replacement);
        if step % 61 == 0 {
            assert_eq!(buffer.len(), expected.len());
            assert_eq!(buffer.to_vec(), expected);
            let mut starts = vec![0];
            for position in 1..=expected.len() {
                if expected[position - 1] == b'\n'
                    || (expected[position - 1] == b'\r' && expected.get(position) != Some(&b'\n'))
                {
                    starts.push(position);
                }
            }
            assert_eq!(buffer.line_count(), starts.len());
            for line in [0, starts.len() / 2, starts.len() - 1] {
                assert_eq!(buffer.line_start(line)?, starts[line]);
            }
            let a = random() % (expected.len() + 1);
            let b = (a + random() % 9000).min(expected.len());
            assert_eq!(buffer.slice(a..b)?.to_vec(), expected[a..b]);
            assert_eq!(buffer.byte(a), expected.get(a).copied());
            assert_eq!(
                buffer.line_index(a)?,
                starts.partition_point(|start| *start <= a) - 1
            );
        }
    }
    assert_eq!(buffer.to_vec(), expected);
    for (snapshot, expected) in snapshots {
        assert_eq!(snapshot.to_vec(), expected);
        let breaks = expected
            .iter()
            .filter(|byte| matches!(byte, b'\r' | b'\n'))
            .count()
            - expected.windows(2).filter(|pair| *pair == b"\r\n").count();
        assert_eq!(snapshot.line_count(), breaks + 1);
        assert_eq!(snapshot.line_index(snapshot.len())?, breaks);
    }
    let before = buffer.to_vec();
    assert_eq!(
        buffer.replace(std::ops::Range { start: 3, end: 2 }, b"x"),
        Err(InvalidByteRange)
    );
    assert_eq!(
        buffer.replace(0..buffer.len() + 1, b"x"),
        Err(InvalidByteRange)
    );
    assert!(buffer.slice(buffer.len() + 1..buffer.len() + 1).is_err());
    assert_eq!(buffer.replace(0..usize::MAX, b"x"), Err(InvalidByteRange));
    assert_eq!(buffer.to_vec(), before);
    Ok(())
}

#[test]
fn captured_byte_roots_share_unedited_storage_and_stream_exact_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    let bytes = (0..1_048_579)
        .map(|index| (index % 256) as u8)
        .collect::<Vec<_>>();
    let mut buffer = ByteBuffer::from_bytes(&bytes);
    let snapshot = buffer.clone();
    let first = snapshot.chunks().next().ok_or("first leaf")?.as_ptr();
    let last = snapshot.chunks().last().ok_or("last leaf")?.as_ptr();
    buffer.replace(500_000..500_001, b"\xff\0\x80")?;
    assert_eq!(buffer.chunks().next().ok_or("first leaf")?.as_ptr(), first);
    assert_eq!(buffer.chunks().last().ok_or("last leaf")?.as_ptr(), last);
    let mut output = Vec::new();
    snapshot.write_to(&mut output)?;
    assert_eq!(output, bytes);
    buffer.replace(0..buffer.len(), b"")?;
    assert!(buffer.is_empty());
    assert_eq!(buffer.byte(0), None);
    assert!(buffer.slice(0..0)?.is_empty());
    assert_eq!(buffer.replace(1..1, b"x"), Err(InvalidByteRange));
    buffer.replace(0..0, b"\xff")?;
    assert_eq!(buffer.to_vec(), b"\xff");
    Ok(())
}
