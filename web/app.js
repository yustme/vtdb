let editor;

// Initialize Monaco Editor
require.config({ paths: { vs: 'https://cdn.jsdelivr.net/npm/monaco-editor@0.45.0/min/vs' } });
require(['vs/editor/editor.main'], function () {
    // Define neobrutalist theme
    monaco.editor.defineTheme('neobrutalist', {
        base: 'vs',
        inherit: true,
        rules: [
            { token: 'comment', foreground: '000000', fontStyle: 'bold' },
            { token: 'keyword', foreground: 'FF6B6B', fontStyle: 'bold' },
            { token: 'string', foreground: '4ECDC4', fontStyle: 'bold' },
            { token: 'number', foreground: '95E1D3', fontStyle: 'bold' },
            { token: 'operator', foreground: '000000', fontStyle: 'bold' },
        ],
        colors: {
            'editor.background': '#FFFFFF',
            'editor.foreground': '#000000',
            'editor.lineHighlightBackground': '#FFE66D',
            'editor.selectionBackground': '#FF6B6B',
            'editorCursor.foreground': '#000000',
            'editorLineNumber.foreground': '#000000',
            'editorLineNumber.activeForeground': '#FF6B6B',
        }
    });
    
    editor = monaco.editor.create(document.getElementById('editor'), {
        value: '-- Enter your SQL query here\nSELECT * FROM users;',
        language: 'sql',
        theme: 'neobrutalist',
        automaticLayout: true,
        minimap: { enabled: false },
        fontSize: 16,
        lineNumbers: 'on',
        roundedSelection: false,
        scrollBeyondLastLine: false,
        fontWeight: '700',
        fontFamily: "'Courier New', 'Courier', monospace",
    });
});

// Execute query
document.getElementById('execute-btn').addEventListener('click', executeQuery);

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
    
    if (!query.trim()) {
        showError('Please enter a SQL query');
        return;
    }
    
    // Show loading state
    resultsContainer.innerHTML = '<div class="loading">Executing query</div>';
    
    // Start timing
    const startTime = performance.now();
    
    try {
        const response = await fetch('/api/execute', {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify({ query: query }),
        });
        
        const data = await response.json();
        
        // Calculate execution time
        const endTime = performance.now();
        const executionTime = endTime - startTime;
        
        if (data.success) {
            if (data.result) {
                displayResults(data.result, executionTime);
            } else {
                showSuccess(`Query executed successfully (no results) - Execution time: ${formatExecutionTime(executionTime)}`);
            }
        } else {
            showError(data.error || 'Unknown error occurred');
        }
    } catch (error) {
        showError('Failed to execute query: ' + error.message);
    }
}

function displayResults(result, executionTime) {
    const resultsContainer = document.getElementById('results-container');
    const MAX_DISPLAY_ROWS = 1000;
    
    if (!result.columns || result.columns.length === 0) {
        resultsContainer.innerHTML = `<div class="info-message">Query executed successfully (no columns) - Execution time: ${formatExecutionTime(executionTime)}</div>`;
        return;
    }
    
    if (!result.rows || result.rows.length === 0) {
        resultsContainer.innerHTML = `<div class="info-message">Query executed successfully (0 rows returned) - Execution time: ${formatExecutionTime(executionTime)}</div>`;
        return;
    }
    
    const totalRows = result.rows.length;
    const rowsToDisplay = result.rows.slice(0, MAX_DISPLAY_ROWS);
    const isTruncated = totalRows > MAX_DISPLAY_ROWS;
    
    let html = '';
    
    // Show execution time at the top
    html += `<div class="info-message" style="margin-bottom: 15px;">
        <strong>⏱️ EXECUTION TIME:</strong> ${formatExecutionTime(executionTime)}
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
    tables.forEach(tableName => {
        html += `<div class="table-item" data-table="${escapeHtml(tableName)}">${escapeHtml(tableName)}</div>`;
    });
    html += '</div>';
    
    tablesList.innerHTML = html;
    
    // Add click handlers to table items
    document.querySelectorAll('.table-item').forEach(item => {
        item.addEventListener('click', function() {
            const tableName = this.getAttribute('data-table');
            
            // Remove selected class from all table items
            document.querySelectorAll('.table-item').forEach(i => {
                i.classList.remove('selected');
            });
            
            // Add selected class to clicked item
            this.classList.add('selected');
            
            // Load table schema
            loadTableSchema(tableName);
            
            // Insert SELECT query into editor
            if (editor) {
                editor.setValue(`SELECT * FROM ${tableName};`);
                editor.focus();
            }
        });
    });
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
            html += `<span class="table-info-column-name">${escapeHtml(column.name)}</span>`;
            html += `<span class="table-info-column-type">${escapeHtml(column.data_type)}</span>`;
            html += `</div>`;
        });
    } else {
        html += '<div class="empty-state"><p>No columns found</p></div>';
    }
    
    html += '</div>';
    
    tableInfoContent.innerHTML = html;
}

