# ShellMounter — Security & Memory Audit

> **Fecha**: 2026-06-06  
> **Versión auditada**: `e7674b2`  
> **Código total**: ~3500 líneas Rust, 28 archivos, 47 tests  
> **Metodología**: Manual whole-codebase review, severidad clasificada

---

## 🔴 CRITICAL — Compromiso inmediato

### C1. check_server_key acepta TODAS las claves (MITM)

**Archivo**: `src/ssh/session.rs:36`
```rust
async fn check_server_key(&mut self, _server_public_key: &ssh_key::PublicKey) -> Result<bool, Self::Error> {
    // TODO: TOFU — For now, accept all
    Ok(true)  // ← Acepta cualquier host key. MITM trivial.
}
```

**Impacto**: Un atacante puede interceptar la conexión SSH y hacerse pasar por el servidor. Todas las credenciales y datos están expuestos.

**Fix**: Implementar TOFU (Trust On First Use):
1. En la primera conexión, guardar `hash(hostname, port) → fingerprint` en `known_hosts` table de SQLite
2. En conexiones posteriores, comparar contra la stored fingerprint
3. Si no coincide, mostrar diálogo al usuario: "Host key changed! Accept?"
4. Usar `ssh_key::PublicKey::fingerprint()` para el hash

---

### C2. MasterKey no se zeroiza al cerrar el vault

**Archivo**: `src/vault/store.rs:100`
```rust
pub fn lock(&mut self) {
    self.master_key = None;  // ← Los 32 bytes quedan en la stack/memoria
}
```

**Impacto**: Un volcado de memoria expone la master key incluso después de "bloquear" el vault. Las llaves SSH quedan descifrables.

**Fix**:
```rust
use zeroize::Zeroize;

pub fn lock(&mut self) {
    if let Some(ref mut key) = self.master_key {
        key.zeroize();  // Sobrescribe con 0x00 antes de soltar
    }
    self.master_key = None;
}
```

---

## 🟠 HIGH — Permite cadena de ataque

### H1. No hay rate limiting en unlock del vault

**Archivo**: `src/ui/app.rs:158` → `vault.unlock(&password)`

**Impacto**: Brute force ilimitado contra la master password. 10M intentos/segundo con Argon2id débil.

**Fix**:
1. Añadir contador de intentos + cooldown exponencial
2. `Vault::unlock()` → `attempts += 1`, `sleep(2^attempts * 100ms)`
3. Bloquear después de 5 intentos por 30 segundos
4. Usar `argon2` con params más agresivos: `m_cost=128*1024` (128 MB), `t_cost=4`, `p_cost=4`

---

### H2. Todo el vault se descifra a memoria de una vez

**Archivo**: `src/vault/store.rs:210` → `load_encrypted()`

**Impacto**: Si el vault tiene 500 llaves SSH, todas están en memoria simultáneamente. Un volcado expone todo.

**Fix**:
1. Descifrar solo el índice (IDs + metadatos), no los blobs
2. Cada blob se descifra bajo demanda con `get(id)`
3. Cache con TTL de 60 segundos, limpiar al hacer `lock()`

---

### H3. El update manifest se descarga sin pinning de certificado

**Archivo**: `src/update/mod.rs:39-45`

**Impacto**: Si el bucket R2 es compromiseado (o hay MITM en la conexión HTTPS de salida), el atacante puede servir un binario malicioso que self-replace la app.

**Fix**:
1. Firmar el manifest con `minisign` o `ssh-keygen -Y sign`
2. Embeber la clave pública en el binario en tiempo de compilación
3. Verificar firma antes de confiar en el manifest
4. Como mínimo, añadir HPKP-like pinning del certificado R2 en el binario

---

## 🟡 MEDIUM — Debilita defensa

### M1. SQLite sin WAL checkpoint ni MaxOpenConns

**Archivo**: `src/db/hosts.rs:44`
```rust
conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
// Falta: PRAGMA wal_autocheckpoint, MaxOpenConns
```

**Fix**:
```rust
conn.execute_batch("PRAGMA wal_autocheckpoint=1000;")?;  // Checkpoint cada 1000 páginas
conn.set_preparet_cache_capacity(32);
// Y en la apertura: limitar a 1 write connection
```

---

### M2. Error de sistema silenciado con unwrap_or_default

**Archivo**: `src/vault/store.rs:159-162`
```rust
let now = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap_or_default()  // ← Si el reloj está antes de 1970, silenciosamente devuelve 0
    .as_secs() as i64;
```

**Impacto**: Timestamps corruptos en la DB si el reloj del sistema está mal. Las entradas parecen de 1970.

**Fix**: `unwrap_or_else(|| { log::warn!("System clock error"); 0 })` — al menos loguear.

---

### M3. Logs sin rotación ni límite de tamaño

**Archivo**: `src/main.rs:33` → `tracing_appender::rolling::daily`

**Impacto**: Los logs crecen indefinidamente. En 6 meses pueden ser GBs.

**Fix**: Añadir cleanup de logs > 7 días en el startup:
```rust
// Borrar logs más viejos que 7 días
for entry in std::fs::read_dir(log_dir)? {
    let entry = entry?;
    if entry.metadata()?.modified()? < std::time::SystemTime::now() - Duration::from_secs(7 * 86400) {
        std::fs::remove_file(entry.path())?;
    }
}
```

---

### M4. No hay validación de hostname/port en el editor

**Archivo**: `src/ui/app.rs:118-141` → `save_host()`

**Impacto**: Se pueden guardar hostnames vacíos, puertos 0, o strings de 10MB como hostname.

**Fix**: Validar en `save_host()`:
- `label.len() > 0 && label.len() < 256`
- `hostname` no vacío, solo caracteres válidos: `[a-zA-Z0-9.-]`
- `port > 0 && port < 65536`
- Limitar longitud de todos los campos a 1024 chars

---

## 🟢 LOW — Mejores prácticas

### L1. `#[allow(dead_code)]` en módulo update
**Archivo**: `src/update/mod.rs` — El campo `min_os` del manifest no se usa. Eliminar o implementar.

### L2. No hay `--data-dir` CLI flag
**Archivo**: `src/main.rs` — El data dir es fijo. Útil para testing: `shellmounter --data-dir /tmp/test-shellmounter`.

### L3. Tests de integración SSH requieren env var manual
**Archivo**: `src/ssh/session.rs:170` — Los tests de SSH se skipean si no hay `SSH_TEST_HOST`. Añadir un `#[ignore]` o un docker-compose con un container SSH para CI.

---

## 🧠 Memory — Prevención de leaks

### MEM1. Plaintext keys en Vec<u8> sin zeroize

**Archivo**: `src/vault/store.rs:189` → `decrypt_blob()` retorna `Vec<u8>`

Cuando el caller hace `drop(data)`, el allocator marca la memoria como libre pero los bytes NO se sobrescriben. Quedan en el heap hasta que otro allocation los reutilice.

**Fix**: Usar `zeroize::Zeroizing<Vec<u8>>` como tipo de retorno, o un wrapper `SensitiveBytes` que zeroiza en Drop.

```rust
use zeroize::Zeroizing;
type SensitiveData = Zeroizing<Vec<u8>>;

fn decrypt_blob(key: &MasterKey, blob: &[u8]) -> Result<SensitiveData, VaultError> {
    // ...
    Ok(Zeroizing::new(plaintext))
}
```

---

### MEM2. Secret.label y Secret.id son String clonados sin necesidad

**Archivo**: `src/vault/store.rs:165-166`

Cada `put()` clona `id` y `label` (heap allocation). Para secrets que no se modifican, es aceptable, pero en un loop de 1000 puts podría fragmentar memoria.

**Fix**: Usar `Arc<str>` si los secrets comparten labels, o aceptar el overhead (es aceptable para la escala esperada: <1000 secrets).

---

### MEM3. La UI recrea el árbol de hosts completo en cada render

**Archivo**: `src/ui/app.rs:270-300` → `.flat_map(...)` por cada grupo

GPUI es retained-mode (no immediate), así que esto es aceptable. Pero la colección `hosts` se clona entera en cada frame si cambia. Para 10000 hosts sería un problema.

**Fix**: Usar `Arc<Vec<Host>>` en `AppState` y clonar el Arc (barato). O usar `gpui::Model` para diffing automático.

---

### MEM4. alacritty_terminal scrollback ilimitado

**Archivo**: `src/terminal/view.rs:95` → `Term::new()` con config default

Por defecto, alacritty guarda 10000 líneas de scrollback. Para sesiones SSH que corren días, esto puede acumular 100MB+ de texto en memoria.

**Fix**: Configurar scrollback a 5000 líneas máximo:
```rust
let mut config = Config::default();
config.scrolling.set_history(5000);
```

---

## 📊 Resumen del audit

| Severidad | Encontrados | Descripción |
|-----------|-------------|-------------|
| 🔴 CRITICAL | 2 | MITM host key bypass + MasterKey sin zeroize |
| 🟠 HIGH | 3 | Brute force unlock + vault completo en RAM + manifest sin firma |
| 🟡 MEDIUM | 4 | SQLite tuning + timestamps + logs rotación + validación host |
| 🟢 LOW | 3 | Dead code + CLI flag + CI tests |
| 🧠 MEMORY | 4 | Zeroize de plaintexts + fragmentation + render + scrollback |

---

## 🚀 Funcionalidades recomendadas (próximas 5)

| # | Funcionalidad | Impacto | Esfuerzo |
|---|--------------|---------|----------|
| 1 | **Importar `~/.ssh/config`** — parsear y migrar hosts existentes | 🔥 Alto | Medio |
| 2 | **Snippets ejecutables** — guardar y enviar comandos frecuentes con Ctrl+Shift+S | 🔥 Alto | Bajo |
| 3 | **Tabs con split horizontal/vertical** — múltiples terminales visibles a la vez | 🔥 Alto | Medio |
| 4 | **Port forwarding UI** — túneles locales/remotos con un toggle | Medio | Bajo |
| 5 | **Notificaciones de inactividad** — alerta cuando un comando largo termina | Medio | Bajo |

### Funcionalidades fase 2 (más ambiciosas)

| # | Funcionalidad | Descripción |
|---|--------------|-------------|
| 6 | **Bastion/Jump hosts** — SSH multi-hop a través de un host intermedio |
| 7 | **Session recording** — grabar sesiones SSH a archivo (asciicast v2) |
| 8 | **Cloud sync** — sincronizar hosts/keys cifrados vía S3/R2/WebDAV |
| 9 | **Team vault** — compartir llaves cifradas con equipo vía age-encryption |
| 10 | **X11 forwarding** — túneles X11 para apps gráficas remotas |
| 11 | **Terminal search** — Ctrl+Shift+F buscar en scrollback |
| 12 | **Quick connect (Ctrl+K)** — palette de comandos estilo VSCode |

---

## 🏷️ Ideas de nombres

El nombre actual `ShellMounter` es descriptivo pero largo y mezcla conceptos con AppMounter. Alternativas:

| Nombre | Significado | Vibe |
|--------|-------------|------|
| **Slate** | Pizarra (de comandos). Corto, memorable. | Profesional, limpio |
| **Telos** | Del griego τέλος (fin, propósito). Terminal + propósito. | Premium, filosófico |
| **Nexus** | Conexión, punto de unión. SSH + SFTP + vault. | Técnico, moderno |
| **Quiver** | Carcaj (de flechas = comandos). Rápido, preciso. | Ágil, agresivo |
| **Hatch** | Escotilla, acceso. Abrir una conexión SSH. | Simple, industrial |
| **Bolt** | Rayo. Conexión rápida. | Velocidad, energía |
| **Forge** | Fragua. Donde se forjan conexiones. | Industrial, robusto |
| **Vault** | Bóveda. El vault es el core. | Seguridad-first |

**Mi recomendación**: **Slate** o **Bolt**. Cortos, fáciles de recordar, no chocan con otros proyectos open source conocidos.

El repo seguiría en `github.com/CoderBotOne/slate` (o `bolt`), y el binario sería `slate` o `bolt`.

---

## ✅ Plan de remediación

| Prioridad | Issues | Tiempo estimado |
|-----------|--------|-----------------|
| 🔴 Ahora | C1, C2 | 2h |
| 🟠 Esta semana | H1, H2, H3 | 4h |
| 🟡 Este sprint | M1-M4 | 2h |
| 🟢 Backlog | L1-L3 | 1h |
| 🧠 Junto con fixes | MEM1, MEM4 | 1h |
