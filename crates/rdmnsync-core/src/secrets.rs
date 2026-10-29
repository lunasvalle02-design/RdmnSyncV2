//! Almacenamiento seguro de API keys en el almacén del sistema
//! (Administrador de credenciales en Windows, Secret Service en Linux).

const SERVICE: &str = "RdmnSync";

pub fn load(instance_id: &str) -> Option<String> {
    keyring::Entry::new(SERVICE, instance_id).ok()?.get_password().ok()
}

pub fn save(instance_id: &str, api_key: &str) -> Result<(), String> {
    keyring::Entry::new(SERVICE, instance_id)
        .and_then(|e| e.set_password(api_key))
        .map_err(|e| format!("No se pudo guardar la API key en el almacén del sistema: {e}"))
}

pub fn delete(instance_id: &str) {
    if let Ok(entry) = keyring::Entry::new(SERVICE, instance_id) {
        let _ = entry.delete_credential();
    }
}
