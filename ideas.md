# ShellMounter — Ideas

Ideas para llevar ShellMounter al siguiente nivel.

## 1. Snippets con variables
Permitir snippets con placeholders tipo `${host}`, `${user}`, `${port}` que se reemplacen automáticamente según el host conectado. Ej: `ssh ${user}@${host} -p ${port}`

## 2. Terminal canvas con colores ANSI
Reemplazar el render actual (divs de texto plano) por un canvas GPUI que renderice secuencias ANSI: colores, bold, cursor, etc. Usar el parser VTE de alacritty_terminal que ya está integrado.

## 3. SFTP drag & drop
Arrastrar archivos entre el panel local y remoto en el SFTP browser. Subir/bajar archivos con drag & drop visual y barra de progreso.

## 4. Multi-terminal en split panels
Dividir el área de terminal en paneles horizontales/verticales para ver múltiples sesiones a la vez. Cada panel es una sesión SSH independiente (ya existe `TerminalPane` y `TerminalLayout` en el código).

## 5. Port forwarding UI visual
Interfaz gráfica para crear/activar/desactivar port forwards. Visualización de túneles activos con estado (conectado/error) y estadísticas de tráfico.

## 6. Session manager + reconnect
Guardar sesiones abiertas al cerrar la app y restaurarlas al abrir. Reconexión automática con reintentos. Configuración de keepalive.

## 7. Host groups con carpetas
Árbol jerárquico de hosts con carpetas anidadas (no solo grupos planos). Ej: `Producción > US > web-1`, `Staging > EU > db-2`.

## 8. Key agent integrado
Actuar como ssh-agent: cargar keys en memoria, ofrecerlas a otras apps vía socket Unix. Las keys del vault se exponen como identidades SSH.

## 9. Command broadcast
Enviar el mismo comando a múltiples hosts simultáneamente. Seleccionar varios hosts con checkboxes y ejecutar un comando en todos a la vez.

## 10. Themes marketplace / import/export
Importar/exportar temas en formato JSON. Compartir configuraciones de hosts, keys y snippets entre equipos. Sincronización opcional vía git o archivo.
