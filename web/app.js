let editor;
let currentAbortController = null;
let executionTimerInterval = null;
let executionStartTime = null;

// Initialize Monaco Editor
require.config({ paths: { vs: 'https://cdn.jsdelivr.net/npm/monaco-editor@0.45.0/min/vs' } });
require(['vs/editor/editor.main'], function () {
    // Define neobrutalist theme
    monaco.editor.defineTheme('neobrutalist', {
        base: 'vs',
        inherit: true,
        rules: [
            { token: 'comment', foreground: '666666', fontStyle: 'bold' },
            { token: 'keyword', foreground: 'CC0000', fontStyle: 'bold' },
            { token: 'string', foreground: '006600', fontStyle: 'bold' },
            { token: 'number', foreground: '0000CC', fontStyle: 'bold' },
            { token: 'operator', foreground: '000000', fontStyle: 'bold' },
            { token: 'type', foreground: '6600CC', fontStyle: 'bold' },
            { token: 'identifier', foreground: '000000', fontStyle: 'normal' },
        ],
        colors: {
            'editor.background': '#FFFFFF',
            'editor.foreground': '#000000',
            'editor.lineHighlightBackground': '#FFE66D',
            'editor.selectionBackground': '#FF6B6B',
            'editorCursor.foreground': '#000000',
            'editorLineNumber.foreground': '#666666',
            'editorLineNumber.activeForeground': '#000000',
            'editorWhitespace.foreground': '#CCCCCC',
        }
    });
    
    editor = monaco.editor.create(document.getElementById('editor'), {
        value: '-- Enter your SQL query here\nSELECT * FROM users;',
        language: 'sql',
        theme: 'neobrutalist',
        automaticLayout: true,
        minimap: { enabled: false },
        fontSize: 13,
        lineNumbers: 'on',
        roundedSelection: false,
        scrollBeyondLastLine: false,
        fontWeight: '700',
        fontFamily: "'Courier New', 'Courier', monospace",
    });
});

// Execute query button handler
document.getElementById('execute-btn').addEventListener('click', function() {
    const executeBtn = document.getElementById('execute-btn');
    if (executeBtn.classList.contains('cancel-mode')) {
        cancelQuery();
    } else {
        executeQuery();
    }
});

// Refresh tables button
document.getElementById('refresh-tables-btn').addEventListener('click', loadTables);

// Execute on Ctrl+Enter
document.addEventListener('keydown', function(e) {
    if (e.ctrlKey && e.key === 'Enter') {
        executeQuery();
    }
});

// Load tables on page load
window.addEventListener('DOMContentLoaded', function() {
    // Wait for Monaco editor to be ready
    setTimeout(loadTables, 500);
    
    // Initialize sidebar resizer
    initSidebarResizer();
});

// Sidebar resizer functionality
function initSidebarResizer() {
    const sidebar = document.getElementById('sidebar');
    const resizer = document.getElementById('resizer');
    
    // Load saved sidebar width from localStorage
    const savedWidth = localStorage.getItem('sidebarWidth');
    if (savedWidth) {
        sidebar.style.width = savedWidth + 'px';
    }
    
    let isResizing = false;
    let startX = 0;
    let startWidth = 0;
    
    resizer.addEventListener('mousedown', function(e) {
        isResizing = true;
        startX = e.clientX;
        startWidth = parseInt(window.getComputedStyle(sidebar).width, 10);
        
        document.addEventListener('mousemove', handleMouseMove);
        document.addEventListener('mouseup', stopResizing);
        
        e.preventDefault();
    });
    
    function handleMouseMove(e) {
        if (!isResizing) return;
        
        const width = startWidth + e.clientX - startX;
        const minWidth = 150;
        const maxWidth = 600;
        
        if (width >= minWidth && width <= maxWidth) {
            sidebar.style.width = width + 'px';
            
            // Update Monaco editor layout if it exists
            if (editor) {
                editor.layout();
            }
        }
    }
    
    function stopResizing() {
        isResizing = false;
        
        // Save sidebar width to localStorage
        const currentWidth = parseInt(window.getComputedStyle(sidebar).width, 10);
        localStorage.setItem('sidebarWidth', currentWidth);
        
        document.removeEventListener('mousemove', handleMouseMove);
        document.removeEventListener('mouseup', stopResizing);
    }
}

async function executeQuery() {
    const query = editor.getValue();
    const resultsContainer = document.getElementById('results-container');
    const executeBtn = document.getElementById('execute-btn');
    
    if (!query.trim()) {
        showError('Please enter a SQL query');
        return;
    }
    
    // Cancel any ongoing query
    if (currentAbortController) {
        currentAbortController.abort();
    }
    
    // Create new abort controller for this query
    currentAbortController = new AbortController();
    
    // Update button to show cancel
    executeBtn.textContent = 'Cancel Query';
    executeBtn.classList.add('cancel-mode');
    
    // Show loading state with timer
    executionStartTime = performance.now();
    startExecutionTimer();
    
    try {
        const response = await fetch('/api/execute', {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify({ query: query }),
            signal: currentAbortController.signal,
        });
        
        const data = await response.json();
        
        // Stop timer
        stopExecutionTimer();
        
        // Calculate execution time
        const endTime = performance.now();
        const executionTime = endTime - executionStartTime;
        
        // Reset button
        resetExecuteButton();
        
        if (data.success) {
            if (data.result) {
                displayResults(data.result, executionTime, data.from_cache || false);
            } else {
                const cacheText = data.from_cache ? ' • Cache used' : '';
                showSuccess(`Query executed successfully (no results) - Execution time: ${formatExecutionTime(executionTime)}${cacheText}`);
            }
        } else {
            showError(data.error || 'Unknown error occurred');
        }
    } catch (error) {
        stopExecutionTimer();
        resetExecuteButton();
        
        if (error.name === 'AbortError') {
            resultsContainer.innerHTML = '<div class="info-message">Query execution cancelled by user</div>';
        } else {
            showError('Failed to execute query: ' + error.message);
        }
    } finally {
        currentAbortController = null;
        executionStartTime = null;
    }
}

function cancelQuery() {
    if (currentAbortController) {
        currentAbortController.abort();
    }
}

function resetExecuteButton() {
    const executeBtn = document.getElementById('execute-btn');
    executeBtn.textContent = 'Execute Query';
    executeBtn.classList.remove('cancel-mode');
}

function startExecutionTimer() {
    const resultsContainer = document.getElementById('results-container');
    
    // Clear any existing timer
    if (executionTimerInterval) {
        clearInterval(executionTimerInterval);
    }
    
    // Update immediately
    updateExecutionTimer();
    
    // Update every 100ms for smooth timer
    executionTimerInterval = setInterval(() => {
        updateExecutionTimer();
    }, 100);
}

function stopExecutionTimer() {
    if (executionTimerInterval) {
        clearInterval(executionTimerInterval);
        executionTimerInterval = null;
    }
}

function updateExecutionTimer() {
    if (!executionStartTime) return;
    
    const resultsContainer = document.getElementById('results-container');
    const elapsed = performance.now() - executionStartTime;
    const elapsedFormatted = formatExecutionTime(elapsed);
    
    const loadingDiv = document.createElement('div');
    loadingDiv.className = 'loading';
    loadingDiv.innerHTML = `
        <div>Executing query...</div>
        <div style="margin-top: 8px; font-size: 0.9em;">Running for: ${elapsedFormatted}</div>
    `;
    
    const cancelBtn = document.createElement('button');
    cancelBtn.className = 'cancel-query-btn';
    cancelBtn.textContent = 'Cancel Query';
    cancelBtn.addEventListener('click', cancelQuery);
    loadingDiv.appendChild(cancelBtn);
    
    resultsContainer.innerHTML = '';
    resultsContainer.appendChild(loadingDiv);
}

function displayResults(result, executionTime, fromCache) {
    const resultsContainer = document.getElementById('results-container');
    const MAX_DISPLAY_ROWS = 1000;
    
    if (!result.columns || result.columns.length === 0) {
        const cacheText = fromCache ? ' • Cache used' : '';
        resultsContainer.innerHTML = `<div class="info-message">Query executed successfully (no columns) - Execution time: ${formatExecutionTime(executionTime)}${cacheText}</div>`;
        return;
    }
    
    if (!result.rows || result.rows.length === 0) {
        const cacheText = fromCache ? ' • Cache used' : '';
        resultsContainer.innerHTML = `<div class="info-message">Query executed successfully (0 rows returned) - Execution time: ${formatExecutionTime(executionTime)}${cacheText}</div>`;
        return;
    }
    
    const totalRows = result.rows.length;
    const rowsToDisplay = result.rows.slice(0, MAX_DISPLAY_ROWS);
    const isTruncated = totalRows > MAX_DISPLAY_ROWS;
    
    let html = '';
    
    // Show execution time and cache status in a single box
    const cacheText = fromCache ? ' • Cache used' : '';
    html += `<div class="info-message" style="margin-bottom: 15px;">
        <strong>⏱️ EXECUTION TIME:</strong> ${formatExecutionTime(executionTime)}${cacheText}
    </div>`;
    
    // Show warning if results are truncated
    if (isTruncated) {
        html += `<div class="info-message" style="margin-bottom: 15px;">
            <strong>⚠️ LARGE RESULT SET:</strong> Query returned ${totalRows} row(s), but only the first ${MAX_DISPLAY_ROWS} row(s) are displayed to prevent UI performance issues.
        </div>`;
    }
    
    html += '<table class="results-table"><thead><tr>';
    
    // Header row
    result.columns.forEach(column => {
        html += `<th>${escapeHtml(column)}</th>`;
    });
    html += '</tr></thead><tbody>';
    
    // Data rows (limited to MAX_DISPLAY_ROWS)
    rowsToDisplay.forEach(row => {
        html += '<tr>';
        row.forEach(cell => {
            const value = formatValue(cell);
            html += `<td>${escapeHtml(value)}</td>`;
        });
        html += '</tr>';
    });
    
    html += '</tbody></table>';
    
    // Add row count info
    if (isTruncated) {
        html += `<div class="info-message" style="margin-top: 15px;">
            Showing ${MAX_DISPLAY_ROWS} of ${totalRows} row(s) returned
        </div>`;
    } else {
        html += `<div class="info-message" style="margin-top: 15px;">${totalRows} row(s) returned</div>`;
    }
    
    resultsContainer.innerHTML = html;
}

function formatExecutionTime(milliseconds) {
    if (milliseconds < 1) {
        return `${(milliseconds * 1000).toFixed(2)} microseconds`;
    } else if (milliseconds < 1000) {
        return `${milliseconds.toFixed(2)} ms`;
    } else {
        const seconds = milliseconds / 1000;
        if (seconds < 60) {
            return `${seconds.toFixed(2)} seconds`;
        } else {
            const minutes = Math.floor(seconds / 60);
            const remainingSeconds = seconds % 60;
            return `${minutes}m ${remainingSeconds.toFixed(2)}s`;
        }
    }
}

function formatValue(value) {
    if (value === null || value === undefined) {
        return 'NULL';
    }
    
    if (typeof value === 'object' && value.type) {
        switch (value.type) {
            case 'Integer':
                return value.value.toString();
            case 'Varchar':
                return value.value;
            case 'Boolean':
                return value.value ? 'TRUE' : 'FALSE';
            case 'Null':
                return 'NULL';
            default:
                return JSON.stringify(value);
        }
    }
    
    return String(value);
}

function showError(message) {
    const resultsContainer = document.getElementById('results-container');
    resultsContainer.innerHTML = `<div class="error-message">${escapeHtml(message)}</div>`;
}

function showSuccess(message) {
    const resultsContainer = document.getElementById('results-container');
    resultsContainer.innerHTML = `<div class="success-message">${escapeHtml(message)}</div>`;
}

function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

// Load tables from API
async function loadTables() {
    const tablesList = document.getElementById('tables-list');
    
    // Show loading state
    tablesList.innerHTML = '<div class="loading">Loading tables</div>';
    
    try {
        const response = await fetch('/api/tables');
        
        if (!response.ok) {
            throw new Error(`HTTP error! status: ${response.status}`);
        }
        
        const text = await response.text();
        if (!text || text.trim() === '') {
            throw new Error('Empty response from server');
        }
        
        const data = JSON.parse(text);
        
        if (data.success) {
            if (data.tables && Array.isArray(data.tables) && data.tables.length > 0) {
                displayTables(data.tables);
            } else {
                tablesList.innerHTML = '<div class="empty-state"><p>No tables found</p></div>';
            }
        } else {
            tablesList.innerHTML = `<div class="error-message">${escapeHtml(data.error || 'Failed to load tables')}</div>`;
        }
    } catch (error) {
        tablesList.innerHTML = `<div class="error-message">Failed to load tables: ${escapeHtml(error.message)}</div>`;
    }
}

// Display tables in the sidebar
function displayTables(tables) {
    const tablesList = document.getElementById('tables-list');
    
    if (!tables || tables.length === 0) {
        tablesList.innerHTML = '<div class="empty-state"><p>No tables found</p></div>';
        return;
    }
    
    let html = '<div class="table-items">';
    tables.forEach(tableInfo => {
        const tableName = tableInfo.name || tableInfo; // Support both old and new format
        const rowCount = tableInfo.row_count !== undefined ? tableInfo.row_count : 0;
        html += `<div class="table-item-wrapper">
            <div class="table-item" data-table="${escapeHtml(tableName)}">
                <span class="table-item-name">${escapeHtml(tableName)} <span class="table-row-count">(${rowCount})</span></span>
                <button class="table-menu-btn" data-table="${escapeHtml(tableName)}" title="Table options">⋯</button>
            </div>
            <div class="table-menu" data-table="${escapeHtml(tableName)}">
                <button class="table-menu-option" data-action="preview" data-table="${escapeHtml(tableName)}">Preview Table</button>
                <button class="table-menu-option" data-action="insert" data-table="${escapeHtml(tableName)}">Place Table Name</button>
            </div>
        </div>`;
    });
    html += '</div>';
    
    tablesList.innerHTML = html;
    
    // Add click handlers to table items (only show schema)
    document.querySelectorAll('.table-item').forEach(item => {
        item.addEventListener('click', function(e) {
            // Don't trigger if clicking on menu button or menu
            if (e.target.classList.contains('table-menu-btn') || 
                e.target.closest('.table-menu-btn') ||
                e.target.closest('.table-menu')) {
                return;
            }
            
            const tableName = this.getAttribute('data-table');
            
            // Remove selected class from all table items
            document.querySelectorAll('.table-item').forEach(i => {
                i.classList.remove('selected');
            });
            
            // Add selected class to clicked item
            this.classList.add('selected');
            
            // Load table schema only
            loadTableSchema(tableName);
        });
    });
    
    // Add click handlers to menu buttons
    document.querySelectorAll('.table-menu-btn').forEach(btn => {
        btn.addEventListener('click', function(e) {
            e.stopPropagation();
            const tableName = this.getAttribute('data-table');
            toggleTableMenu(tableName);
        });
    });
    
    // Add click handlers to menu options
    document.querySelectorAll('.table-menu-option').forEach(option => {
        option.addEventListener('click', function(e) {
            e.stopPropagation();
            const action = this.getAttribute('data-action');
            const tableName = this.getAttribute('data-table');
            handleTableMenuAction(action, tableName);
        });
    });
    
    // Close menus when clicking outside
    document.addEventListener('click', function(e) {
        if (!e.target.closest('.table-item-wrapper')) {
            closeAllTableMenus();
        }
    });
}

// Toggle table menu visibility
function toggleTableMenu(tableName) {
    // Close all other menus first
    closeAllTableMenus();
    
    // Toggle the clicked menu (tableName is already escaped in HTML)
    const menu = document.querySelector(`.table-menu[data-table="${tableName}"]`);
    if (menu) {
        menu.classList.toggle('open');
    }
}

// Close all table menus
function closeAllTableMenus() {
    document.querySelectorAll('.table-menu').forEach(menu => {
        menu.classList.remove('open');
    });
}

// Handle table menu actions
function handleTableMenuAction(action, tableName) {
    closeAllTableMenus();
    
    if (action === 'preview') {
        previewTable(tableName);
    } else if (action === 'insert') {
        insertTableName(tableName);
    }
}

// Preview table (SELECT * FROM table LIMIT 50)
async function previewTable(tableName) {
    if (!editor) return;
    
    const query = `SELECT * FROM ${tableName} LIMIT 50`;
    editor.setValue(query);
    editor.focus();
    
    // Execute the query
    await executeQuery();
}

// Insert table name at cursor position
function insertTableName(tableName) {
    if (!editor) return;
    
    const selection = editor.getSelection();
    const range = new monaco.Range(
        selection.startLineNumber,
        selection.startColumn,
        selection.endLineNumber,
        selection.endColumn
    );
    
    editor.executeEdits('insert-table-name', [{
        range: range,
        text: tableName
    }]);
    
    editor.focus();
}

// Load table schema from API
async function loadTableSchema(tableName) {
    const tableInfo = document.getElementById('table-info');
    const tableInfoContent = tableInfo.querySelector('.table-info-content');
    
    // Show loading state
    tableInfoContent.innerHTML = '<div class="loading">Loading schema</div>';
    
    try {
        const response = await fetch(`/api/table/${encodeURIComponent(tableName)}`);
        
        if (!response.ok) {
            throw new Error(`HTTP error! status: ${response.status}`);
        }
        
        const text = await response.text();
        if (!text || text.trim() === '') {
            throw new Error('Empty response from server');
        }
        
        const data = JSON.parse(text);
        
        if (data.success && data.schema) {
            displayTableSchema(data.schema);
        } else {
            tableInfoContent.innerHTML = `<div class="error-message">${escapeHtml(data.error || 'Failed to load table schema')}</div>`;
        }
    } catch (error) {
        tableInfoContent.innerHTML = `<div class="error-message">Failed to load table schema: ${escapeHtml(error.message)}</div>`;
    }
}

// Display table schema information
function displayTableSchema(schema) {
    const tableInfo = document.getElementById('table-info');
    const tableInfoContent = tableInfo.querySelector('.table-info-content');
    
    let html = `<h3 class="table-info-title">${escapeHtml(schema.name)}</h3>`;
    html += '<div class="table-info-columns">';
    
    if (schema.columns && schema.columns.length > 0) {
        schema.columns.forEach(column => {
            html += `<div class="table-info-column">`;
            html += `<span class="table-info-column-name">${escapeHtml(column.name)}`;
            if (column.index_type) {
                html += ` <span class="index-badge index-${column.index_type.toLowerCase()}" title="Indexed with ${column.index_type}">[${column.index_type}]</span>`;
            }
            html += `</span>`;
            html += `<span class="table-info-column-type">${escapeHtml(column.data_type)}</span>`;
            html += `</div>`;
        });
    } else {
        html += '<div class="empty-state"><p>No columns found</p></div>';
    }
    
    html += '</div>';
    
    // Add storage size information if available
    if (schema.stats) {
        const sizeMB = (schema.stats.storage_size_bytes / (1024 * 1024)).toFixed(2);
        html += `<div class="table-info-storage">
            <div class="table-info-storage-label">Storage Size:</div>
            <div class="table-info-storage-value">${sizeMB} MB</div>
        </div>`;
    }
    
    tableInfoContent.innerHTML = html;
}

// Format bytes to human-readable size
function formatBytes(bytes) {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return (bytes / Math.pow(k, i)).toFixed(2) + ' ' + sizes[i];
}

