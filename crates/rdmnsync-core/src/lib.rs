//! Motor de RdmnSync, independiente de la interfaz: cliente REST de Redmine, emparejamiento,
//! plan y ejecución de la sincronización, igualado de issues, configuración y API keys.
//!
//! Todo es bloqueante; quien lo use desde una interfaz debe llamarlo en hilos de fondo.

pub mod dates;
pub mod format;
pub mod issue_sync;
pub mod matching;
pub mod redmine;
pub mod secrets;
pub mod store;
pub mod sync;
