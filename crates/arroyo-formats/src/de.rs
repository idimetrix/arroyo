use crate::avro::de;
use arrow_array::builder::{ArrayBuilder, StringBuilder, TimestampNanosecondBuilder};
use arrow_array::{RecordBatch, StringArray};
use arroyo_rpc::df::ArroyoSchema;
use arroyo_rpc::formats::{AvroFormat, BadData, Format, Framing, FramingMethod, JsonFormat};
use arroyo_rpc::schema_resolver::{FailingSchemaResolver, FixedSchemaResolver, SchemaResolver};
use arroyo_types::{should_flush, to_nanos, RawJson, SourceError};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Instant, SystemTime};
use tokio::sync::Mutex;

struct RawJsonIterator {
    offset: usize,
    rows: usize,
    column: StringArray,
}

impl Iterator for RawJsonIterator {
    type Item = RawJson;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset == self.rows {
            return None;
        }
        let val = self.column.value(self.offset);
        self.offset += 1;
        Some(RawJson {
            value: val.to_string(),
        })
    }
}

pub struct FramingIterator<'a> {
    framing: Option<Arc<Framing>>,
    buf: &'a [u8],
    offset: usize,
}

impl<'a> FramingIterator<'a> {
    pub fn new(framing: Option<Arc<Framing>>, buf: &'a [u8]) -> Self {
        Self {
            framing,
            buf,
            offset: 0,
        }
    }
}

impl<'a> Iterator for FramingIterator<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.buf.len() {
            return None;
        }

        match &self.framing {
            Some(framing) => {
                match &framing.method {
                    FramingMethod::Newline(newline) => {
                        let end = memchr::memchr('\n' as u8, &self.buf[self.offset..])
                            .map(|i| self.offset + i)
                            .unwrap_or(self.buf.len());

                        let prev = self.offset;
                        self.offset = end + 1;

                        // enforce max len if set
                        let length =
                            (end - prev).min(newline.max_line_length.unwrap_or(u64::MAX) as usize);

                        Some(&self.buf[prev..(prev + length)])
                    }
                }
            }
            None => {
                self.offset = self.buf.len();
                Some(self.buf)
            }
        }
    }
}

pub struct ArrowDeserializer {
    format: Arc<Format>,
    framing: Option<Arc<Framing>>,
    schema: ArroyoSchema,
    bad_data: BadData,
    json_decoder: Option<(arrow::json::reader::Decoder, TimestampNanosecondBuilder)>,
    buffered_count: usize,
    buffered_since: Instant,
    schema_registry: Arc<Mutex<HashMap<u32, apache_avro::schema::Schema>>>,
    schema_resolver: Arc<dyn SchemaResolver + Sync>,
}

impl ArrowDeserializer {
    pub fn new(
        format: Format,
        schema: ArroyoSchema,
        framing: Option<Framing>,
        bad_data: BadData,
    ) -> Self {
        let resolver = if let Format::Avro(AvroFormat {
            reader_schema: Some(schema),
            ..
        }) = &format
        {
            Arc::new(FixedSchemaResolver::new(0, schema.clone().into()))
                as Arc<dyn SchemaResolver + Sync>
        } else {
            Arc::new(FailingSchemaResolver::new()) as Arc<dyn SchemaResolver + Sync>
        };

        Self::with_schema_resolver(format, framing, schema, bad_data, resolver)
    }

    pub fn with_schema_resolver(
        format: Format,
        framing: Option<Framing>,
        schema: ArroyoSchema,
        bad_data: BadData,
        schema_resolver: Arc<dyn SchemaResolver + Sync>,
    ) -> Self {
        Self {
            json_decoder: matches!(
                format,
                Format::Json(..)
                    | Format::Avro(AvroFormat {
                        into_unstructured_json: false,
                        ..
                    })
            )
            .then(|| {
                // exclude the timestamp field
                (
                    arrow_json::reader::ReaderBuilder::new(Arc::new(
                        schema.schema_without_timestamp(),
                    ))
                    .with_limit_to_batch_size(false)
                    .with_strict_mode(false)
                    .build_decoder()
                    .unwrap(),
                    TimestampNanosecondBuilder::new(),
                )
            }),
            format: Arc::new(format),
            framing: framing.map(|f| Arc::new(f)),
            schema,
            schema_registry: Arc::new(Mutex::new(HashMap::new())),
            bad_data,
            schema_resolver,
            buffered_count: 0,
            buffered_since: Instant::now(),
        }
    }

    pub async fn deserialize_slice(
        &mut self,
        buffer: &mut Vec<Box<dyn ArrayBuilder>>,
        msg: &[u8],
        timestamp: SystemTime,
    ) -> Vec<SourceError> {
        match &*self.format {
            Format::Avro(_) => self.deserialize_slice_avro(buffer, msg, timestamp).await,
            _ => FramingIterator::new(self.framing.clone(), msg)
                .map(|t| self.deserialize_single(buffer, t, timestamp))
                .filter_map(|t| t.err())
                .collect(),
        }
    }

    pub fn should_flush(&self) -> bool {
        should_flush(self.buffered_count, self.buffered_since)
    }

    pub fn flush_buffer(&mut self) -> Option<Result<RecordBatch, SourceError>> {
        let (decoder, timestamp) = self.json_decoder.as_mut()?;
        self.buffered_since = Instant::now();
        self.buffered_count = 0;
        Some(
            decoder
                .flush()
                .map_err(|e| SourceError::bad_data(format!("JSON does not match schema: {:?}", e)))
                .transpose()?
                .map(|batch| {
                    let mut columns = batch.columns().to_vec();
                    columns.insert(self.schema.timestamp_index, Arc::new(timestamp.finish()));
                    RecordBatch::try_new(self.schema.schema.clone(), columns).unwrap()
                }),
        )
    }

    fn deserialize_single(
        &mut self,
        buffer: &mut Vec<Box<dyn ArrayBuilder>>,
        msg: &[u8],
        timestamp: SystemTime,
    ) -> Result<(), SourceError> {
        match &*self.format {
            Format::RawString(_)
            | Format::Json(JsonFormat {
                unstructured: true, ..
            }) => {
                self.deserialize_raw_string(buffer, msg);
                add_timestamp(buffer, self.schema.timestamp_index, timestamp);
            }
            Format::Json(json) => {
                let msg = if json.confluent_schema_registry {
                    &msg[5..]
                } else {
                    msg
                };

                let Some((decoder, timestamp_builder)) = &mut self.json_decoder else {
                    panic!("json decoder not initialized");
                };

                decoder
                    .decode(msg)
                    .map_err(|e| SourceError::bad_data(format!("invalid JSON: {:?}", e)))?;
                timestamp_builder.append_value(to_nanos(timestamp) as i64);
                self.buffered_count += 1;
            }
            Format::Avro(_) => unreachable!("this should not be called for avro"),
            Format::Parquet(_) => todo!("parquet is not supported as an input format"),
        }

        Ok(())
    }

    pub async fn deserialize_slice_avro<'a>(
        &mut self,
        builders: &mut Vec<Box<dyn ArrayBuilder>>,
        mut msg: &'a [u8],
        timestamp: SystemTime,
    ) -> Vec<SourceError> {
        let Format::Avro(format) = &*self.format else {
            unreachable!("not avro");
        };

        let messages = match de::avro_messages(
            format,
            &self.schema_registry,
            &self.schema_resolver,
            &mut msg,
        )
        .await
        {
            Ok(messages) => messages,
            Err(e) => {
                return vec![e];
            }
        };

        let into_json = format.into_unstructured_json;
        let errors = messages
            .into_iter()
            .map(|record| {
                let value = record.map_err(|e| {
                    SourceError::bad_data(format!("failed to deserialize from avro: {:?}", e))
                })?;

                if into_json {
                    let (idx, _) = self
                        .schema
                        .schema
                        .column_with_name("value")
                        .expect("no 'value' column for unstructed avro");
                    let array = builders[idx]
                        .as_any_mut()
                        .downcast_mut::<StringBuilder>()
                        .expect("'value' column has incorrect type");

                    array.append_value(de::avro_to_json(value).to_string());
                    add_timestamp(builders, self.schema.timestamp_index, timestamp);
                    self.buffered_count += 1;
                } else {
                    // for now round-trip through json in order to handle unsupported avro features
                    // as that allows us to rely on raw json deserialization
                    let json = de::avro_to_json(value).to_string();

                    let Some((decoder, timestamp_builder)) = &mut self.json_decoder else {
                        panic!("json decoder not initialized");
                    };

                    decoder
                        .decode(json.as_bytes())
                        .map_err(|e| SourceError::bad_data(format!("invalid JSON: {:?}", e)))?;
                    self.buffered_count += 1;
                    timestamp_builder.append_value(to_nanos(timestamp) as i64);
                }

                Ok(())
            })
            .filter_map(|r: Result<(), SourceError>| r.err())
            .collect();

        errors
    }

    fn deserialize_raw_string(&mut self, buffer: &mut Vec<Box<dyn ArrayBuilder>>, msg: &[u8]) {
        let (col, _) = self
            .schema
            .schema
            .column_with_name("value")
            .expect("no 'value' column for RawString format");
        buffer[col]
            .as_any_mut()
            .downcast_mut::<StringBuilder>()
            .expect("'value' column has incorrect type")
            .append_value(String::from_utf8_lossy(msg));
    }

    pub fn bad_data(&self) -> &BadData {
        &self.bad_data
    }
}

pub(crate) fn add_timestamp(
    builder: &mut Vec<Box<dyn ArrayBuilder>>,
    idx: usize,
    timestamp: SystemTime,
) {
    builder[idx]
        .as_any_mut()
        .downcast_mut::<TimestampNanosecondBuilder>()
        .expect("_timestamp column has incorrect type")
        .append_value(to_nanos(timestamp) as i64);
}

#[cfg(test)]
mod tests {
    use crate::de::FramingIterator;
    use arroyo_rpc::formats::{Framing, FramingMethod, NewlineDelimitedFraming};
    use std::sync::Arc;

    #[test]
    fn test_line_framing() {
        let framing = Some(Arc::new(Framing {
            method: FramingMethod::Newline(NewlineDelimitedFraming {
                max_line_length: None,
            }),
        }));

        let result: Vec<_> = FramingIterator::new(framing.clone(), "one block".as_bytes())
            .map(|t| String::from_utf8(t.to_vec()).unwrap())
            .collect();

        assert_eq!(vec!["one block".to_string()], result);

        let result: Vec<_> = FramingIterator::new(
            framing.clone(),
            "one block\ntwo block\nthree block".as_bytes(),
        )
        .map(|t| String::from_utf8(t.to_vec()).unwrap())
        .collect();

        assert_eq!(
            vec![
                "one block".to_string(),
                "two block".to_string(),
                "three block".to_string(),
            ],
            result
        );

        let result: Vec<_> = FramingIterator::new(
            framing.clone(),
            "one block\ntwo block\nthree block\n".as_bytes(),
        )
        .map(|t| String::from_utf8(t.to_vec()).unwrap())
        .collect();

        assert_eq!(
            vec![
                "one block".to_string(),
                "two block".to_string(),
                "three block".to_string(),
            ],
            result
        );
    }

    #[test]
    fn test_max_line_length() {
        let framing = Some(Arc::new(Framing {
            method: FramingMethod::Newline(NewlineDelimitedFraming {
                max_line_length: Some(5),
            }),
        }));

        let result: Vec<_> =
            FramingIterator::new(framing, "one block\ntwo block\nwhole".as_bytes())
                .map(|t| String::from_utf8(t.to_vec()).unwrap())
                .collect();

        assert_eq!(
            vec![
                "one b".to_string(),
                "two b".to_string(),
                "whole".to_string(),
            ],
            result
        );
    }
}
