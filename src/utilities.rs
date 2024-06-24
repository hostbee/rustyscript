use crate::{Error, FunctionArguments, serde_json};
use crate::traits::ToModuleSpecifier;

/// The lack of any arguments - used to simplify calling functions
/// Prevents you from needing to specify the type using ::<serde_json::Value>
pub const EMPTY_ARGS: &'static FunctionArguments = &[];

pub fn arg<A>(value: A) -> Result<serde_json::Value, Error>
where
    A: serde::Serialize,
{
    Ok(serde_json::to_value(value)?)
}

pub fn into_arg<A>(value: A) -> serde_json::Value
where
    serde_json::Value: From<A>,
{
    serde_json::Value::from(value)
}

#[cfg(feature = "sync")]
pub mod sync {
    use crate::{Error, Module, ModuleWrapper, Runtime};
    use crate::traits::ToModuleSpecifier;

    /// Evaluate a piece of non-ECMAScript-module JavaScript code
    /// Effects on the global scope will not persist
    /// For a persistant variant, see [Runtime::eval]
    ///
    /// # Arguments
    /// * `javascript` - A single javascript expression
    ///
    /// # Returns
    /// A `Result` containing the deserialized result of the expression if successful,
    /// or an error if execution fails, or the result cannot be deserialized.
    ///
    /// # Example
    ///
    /// ```rust
    /// let result: i64 = rustyscript::evaluate("5 + 5").expect("The expression was invalid!");
    /// assert_eq!(10, result);
    /// ```
    pub fn evaluate<T>(javascript: &str) -> Result<T, Error>
    where
        T: deno_core::serde::de::DeserializeOwned,
    {
        let mut runtime = Runtime::new(Default::default())?;
        runtime.eval(javascript)
    }

    /// Validates the syntax of some JS
    ///
    /// # Arguments
    /// * `javascript` - A snippet of JS code
    ///
    /// # Returns
    /// A `Result` containing a boolean determining the validity of the JS,
    /// or an error if something went wrong.
    ///
    /// # Example
    ///
    /// ```rust
    /// assert!(rustyscript::validate("5 + 5").expect("Something went wrong!"));
    /// ```
    pub fn validate(javascript: &str) -> Result<bool, Error> {
        let module = Module::new("test.js", javascript);
        let mut runtime = Runtime::new(Default::default())?;
        match runtime.load_modules(&module, vec![]) {
            Ok(_) => Ok(true),
            Err(Error::Runtime(_)) => Ok(false),
            Err(Error::JsError(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Imports a JS module into a new runtime
    ///
    /// # Arguments
    /// * `path` - Path to the JS module to import
    ///
    /// # Returns
    /// A `Result` containing a handle to the imported module,
    /// or an error if something went wrong.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let mut module = rustyscript::import("js/my_module.js").expect("Something went wrong!");
    /// ```
    pub fn import(path: &str) -> Result<ModuleWrapper, Error> {
        ModuleWrapper::new_from_file(path, Default::default())
    }

    /// Resolve a path to absolute path
    ///
    /// # Arguments
    /// * `path` - A path
    ///
    /// # Example
    ///
    /// ```rust
    /// let full_path = rustyscript::resolve_path("test.js").expect("Something went wrong!");
    /// assert!(full_path.ends_with("test.js"));
    /// ```
    pub fn resolve_path(path: &str) -> Result<String, Error> {
        Ok(path.to_module_specifier()?.to_string())
    }

    #[cfg(test)]
    mod test_runtime {
        use super::*;
        use deno_core::{futures::FutureExt, serde_json};
        use crate::{async_callback, evaluate, resolve_path, sync_callback, validate};

        #[test]
        fn test_callback() {
            let add = sync_callback!(|a: i64, b: i64| { Ok::<i64, Error>(a + b) });

            let add2 = async_callback!(|a: i64, b: i64| { async move { Ok::<i64, Error>(a + b) } });

            let args = vec![
                serde_json::Value::Number(5.into()),
                serde_json::Value::Number(5.into()),
            ];
            let result = add(&args).unwrap();
            assert_eq!(serde_json::Value::Number(10.into()), result);

            let result = add2(args).now_or_never().unwrap().unwrap();
            assert_eq!(serde_json::Value::Number(10.into()), result);
        }

        #[test]
        fn test_evaluate() {
            assert_eq!(5, evaluate::<i64>("3 + 2").expect("invalid expression"));
            evaluate::<i64>("a5; 3 + 2").expect_err("Expected an error");
        }

        #[test]
        fn test_validate() {
            assert_eq!(true, validate("3 + 2").expect("invalid expression"));
            assert_eq!(false, validate("5;+-").expect("invalid expression"));
        }

        #[test]
        fn test_resolve_path() {
            assert!(resolve_path("test.js")
                .expect("invalid path")
                .ends_with("test.js"));
        }
    }
}

#[macro_use]
mod runtime_macros {
    /// Map a series of values to a slice of `serde_json::Value` objects
    /// that javascript functions can understand
    /// # Example
    /// ```rust
    /// use rustyscript::{ Runtime, RuntimeOptions, Module, json_args };
    /// use std::time::Duration;
    ///
    /// # fn main() -> Result<(), rustyscript::Error> {
    /// let module = Module::new("test.js", "
    ///     function load(a, b) {
    ///         console.log(`Hello world: a=${a}, b=${b}`);
    ///     }
    ///     rustyscript.register_entrypoint(load);
    /// ");
    ///
    /// Runtime::execute_module(
    ///     &module, vec![],
    ///     Default::default(),
    ///     json_args!("test", 5)
    /// )?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    #[macro_export]
    macro_rules! json_args {
        ($($arg:expr),+) => {
            &[
                $($crate::utilities::into_arg($arg)),+
            ]
        };

        () => {
            $crate::utilities::EMPTY_ARGS
        };
    }

    #[macro_export]
    macro_rules! json_args_vec {
        ($($arg:expr),+) => {
            (&[
                $($crate::utilities::into_arg($arg)),+
            ]).to_vec()
        };

        () => {
            vec![]
        };
    }

    /// A simple helper macro to create a callback for use with `Runtime::register_function`
    /// Takes care of deserializing arguments and serializing the result
    ///
    /// # Example
    /// ```rust
    /// use rustyscript::{ Error, sync_callback };
    /// let add = sync_callback!(
    ///     (a: i64, b: i64) {
    ///         Ok::<i64, Error>(a + b)
    ///     }
    /// );
    /// ```
    #[macro_export]
    macro_rules! sync_callback {
        (|$($arg:ident: $arg_ty:ty),*| $body:block) => {
            |args: &[$crate::serde_json::Value]| {
                let mut args = args.iter();
                $(
                    let $arg: $arg_ty = match args.next() {
                        Some(arg) => $crate::serde_json::from_value(arg.clone())?,
                        None => return Err($crate::Error::Runtime("Invalid number of arguments".to_string())),
                    };
                )*
                let result = $body?;
                Ok($crate::serde_json::Value::try_from(result).map_err(|e| $crate::Error::Runtime(e.to_string()))?)
            }
        }
    }

    /// A simple helper macro to create a callback for use with `Runtime::register_async_function`
    /// Takes care of deserializing arguments and serializing the result
    ///
    /// # Example
    /// ```rust
    /// use rustyscript::{ Error, sync_callback };
    /// let add = async_callback!(
    ///     (a: i64, b: i64) {
    ///         Ok::<i64, Error>(a + b)
    ///     }
    /// );
    /// ```
    #[macro_export]
    macro_rules! async_callback {
        (|$($arg:ident: $arg_ty:ty),*| $body:block) => {
            |args: Vec<$crate::serde_json::Value>| Box::pin(async move {
                let mut args = args.iter();
                $(
                    let $arg: $arg_ty = match args.next() {
                        Some(arg) => $crate::serde_json::from_value(arg.clone()).map_err(|e| $crate::Error::Runtime(e.to_string()))?,
                        None => return Err($crate::Error::Runtime("Invalid number of arguments".to_string())),
                    };
                )*

                // Now consume the future to inject JSON serialization
                let result = $body.await?;
                $crate::serde_json::Value::try_from(result).map_err(|e| $crate::Error::Runtime(e.to_string()))
            })
        }
    }
}


