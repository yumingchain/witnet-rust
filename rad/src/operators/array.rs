use crate::error::RadError;
use crate::reducers::{self, RadonReducers};
use crate::script::{execute_radon_script, unpack_radon_call};
use crate::types::{array::RadonArray, integer::RadonInteger, RadonType, RadonTypes};

use num_traits::FromPrimitive;
use serde_cbor::value::{from_value, Value};
use std::clone::Clone;

pub fn count(input: &RadonArray) -> RadonInteger {
    RadonInteger::from(input.value().len() as i128)
}

pub fn reduce(input: &RadonArray, args: &[Value]) -> Result<RadonTypes, RadError> {
    let wrong_args = || RadError::WrongArguments {
        input_type: "RadonArray".to_string(),
        operator: "Reduce".to_string(),
        args: args.to_vec(),
    };

    let arg = args.first().ok_or_else(wrong_args)?.to_owned();
    let reducer_integer = from_value::<i64>(arg).map_err(|_| wrong_args())?;
    let reducer_code = RadonReducers::from_i64(reducer_integer).ok_or_else(wrong_args)?;

    reducers::reduce(input, reducer_code)
}

pub fn get(input: &RadonArray, args: &[Value]) -> Result<RadonTypes, RadError> {
    let wrong_args = || RadError::WrongArguments {
        input_type: "RadonArray".to_string(),
        operator: "Reduce".to_string(),
        args: args.to_vec(),
    };

    let not_found = |index: i32| RadError::ArrayIndexNotFound { index };

    let arg = args.first().ok_or_else(wrong_args)?.to_owned();
    let index = from_value::<i32>(arg).map_err(|_| wrong_args())?;

    input
        .value()
        .get(index as usize)
        .map(Clone::clone)
        .ok_or_else(|| not_found(index))
}

pub fn map(input: &RadonArray, args: &[Value]) -> Result<RadonTypes, RadError> {
    let mut subscript = vec![];
    for arg in args {
        subscript.push(unpack_radon_call(arg)?)
    }

    let mut result = vec![];
    for item in input.value() {
        result.push(execute_radon_script(item, subscript.as_slice())?);
    }

    Ok(RadonArray::from(result).into())
}

pub fn filter(input: &RadonArray, args: &[Value]) -> Result<RadonTypes, RadError> {
    let mut subscript = vec![];
    for arg in args {
        subscript.push(unpack_radon_call(arg)?)
    }

    let mut result = vec![];
    for item in input.value() {
        match execute_radon_script(item.clone(), subscript.as_slice())? {
            RadonTypes::Boolean(boolean) => {
                if boolean.value() {
                    result.push(item);
                }
            }
            value => Err(RadError::ArrayFilterWrongSubscript {
                value: value.to_string(),
            })?,
        }
    }

    Ok(RadonArray::from(result).into())
}

#[test]
fn test_array_count() {
    use crate::types::float::RadonFloat;

    let input = &RadonArray::from(vec![
        RadonFloat::from(1f64).into(),
        RadonFloat::from(2f64).into(),
    ]);

    let empty = &RadonArray::from(vec![]);

    assert_eq!(count(&input), RadonInteger::from(2));
    assert_eq!(count(&empty), RadonInteger::from(0));
}

#[test]
fn test_reduce_no_args() {
    use crate::types::float::RadonFloat;

    let input = &RadonArray::from(vec![
        RadonFloat::from(1f64).into(),
        RadonFloat::from(2f64).into(),
    ]);
    let args = &[];

    let result = reduce(input, args);

    assert_eq!(
        &result.unwrap_err().to_string(),
        "Wrong `RadonArray::Reduce()` arguments: `[]`"
    );
}

#[test]
fn test_reduce_wrong_args() {
    use crate::types::float::RadonFloat;

    let input = &RadonArray::from(vec![
        RadonFloat::from(1f64).into(),
        RadonFloat::from(2f64).into(),
    ]);
    let args = &[Value::Text(String::from("wrong"))]; // This is RadonReducers::AverageMean

    let result = reduce(input, args);

    assert_eq!(
        &result.unwrap_err().to_string(),
        "Wrong `RadonArray::Reduce()` arguments: `[Text(\"wrong\")]`"
    );
}

#[test]
fn test_reduce_unknown_reducer() {
    use crate::types::float::RadonFloat;

    let input = &RadonArray::from(vec![
        RadonFloat::from(1f64).into(),
        RadonFloat::from(2f64).into(),
    ]);
    let args = &[Value::Integer(-1)]; // This doesn't match any reducer code in RadonReducers

    let result = reduce(input, args);

    assert_eq!(
        &result.unwrap_err().to_string(),
        "Wrong `RadonArray::Reduce()` arguments: `[Integer(-1)]`"
    );
}

#[test]
fn test_reduce_average_mean_float() {
    use crate::types::float::RadonFloat;

    let input = &RadonArray::from(vec![
        RadonFloat::from(1f64).into(),
        RadonFloat::from(2f64).into(),
    ]);
    let args = &[Value::Integer(0x03)]; // This is RadonReducers::AverageMean
    let expected = RadonTypes::from(RadonFloat::from(1.5f64));

    let output = reduce(input, args).unwrap();

    assert_eq!(output, expected);
}

#[test]
fn test_map_integer_greater_than() {
    use crate::operators::RadonOpCodes::IntegerGreaterThan;
    use crate::types::boolean::RadonBoolean;

    let input = RadonArray::from(vec![
        RadonInteger::from(2).into(),
        RadonInteger::from(6).into(),
    ]);
    let script = vec![Value::Array(vec![
        Value::Integer(IntegerGreaterThan as i128),
        Value::Integer(4),
    ])];
    let output = map(&input, &script).unwrap();

    let expected = RadonTypes::Array(RadonArray::from(vec![
        RadonBoolean::from(false).into(),
        RadonBoolean::from(true).into(),
    ]));

    assert_eq!(output, expected)
}

#[test]
fn test_filter_integer_greater_than() {
    use crate::operators::RadonOpCodes::IntegerGreaterThan;
    use crate::types::integer::RadonInteger;

    let input = RadonArray::from(vec![
        RadonInteger::from(2).into(),
        RadonInteger::from(6).into(),
    ]);
    let script = vec![Value::Array(vec![
        Value::Integer(IntegerGreaterThan as i128),
        Value::Integer(4),
    ])];
    let output = filter(&input, &script).unwrap();

    let expected = RadonTypes::Array(RadonArray::from(vec![RadonInteger::from(6).into()]));

    assert_eq!(output, expected)
}

#[test]
fn test_filter_negative() {
    use crate::operators::RadonOpCodes::IntegerMultiply;
    use crate::types::integer::RadonInteger;

    let input = RadonArray::from(vec![
        RadonInteger::from(2).into(),
        RadonInteger::from(6).into(),
    ]);
    let script = vec![Value::Array(vec![
        Value::Integer(IntegerMultiply as i128),
        Value::Integer(4),
    ])];
    let result = filter(&input, &script);

    assert_eq!(
        &result.unwrap_err().to_string(),
        "ArrayFilter subscript output was not RadonBoolean (was `RadonTypes::RadonInteger(8)`)"
    );
}
