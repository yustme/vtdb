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
});

async function executeQuery() {
    const query = editor.getValue();
    const resultsContainer = document.getElementById('results-container');
    
    if (!query.trim()) {
        showError('Please enter a SQL query');
        return;
    }
    
    // Show loading state
    resultsContainer.innerHTML = '<div class="loading">Executing query</div>';
    
    try {
        const response = await fetch('/api/execute', {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify({ query: query }),
        });
        
        const data = await response.json();
        
        if (data.success) {
            if (data.result) {
                displayResults(data.result);
            } else {
                showSuccess('Query executed successfully (no results)');
            }
        } else {
            showError(data.error || 'Unknown error occurred');
        }
    } catch (error) {
        showError('Failed to execute query: ' + error.message);
    }
}

function displayResults(result) {
    const resultsContainer = document.getElementById('results-container');
    
    if (!result.columns || result.columns.length === 0) {
        resultsContainer.innerHTML = '<div class="info-message">Query executed successfully (no columns)</div>';
        return;
    }
    
    if (!result.rows || result.rows.length === 0) {
        resultsContainer.innerHTML = '<div class="info-message">Query executed successfully (0 rows returned)</div>';
        return;
    }
    
    let html = '<table class="results-table"><thead><tr>';
    
    // Header row
    result.columns.forEach(column => {
        html += `<th>${escapeHtml(column)}</th>`;
    });
    html += '</tr></thead><tbody>';
    
    // Data rows
    result.rows.forEach(row => {
        html += '<tr>';
        row.forEach(cell => {
            const value = formatValue(cell);
            html += `<td>${escapeHtml(value)}</td>`;
        });
        html += '</tr>';
    });
    
    html += '</tbody></table>';
    
    // Add row count info
    html += `<div class="info-message" style="margin-top: 10px;">${result.rows.length} row(s) returned</div>`;
    
    resultsContainer.innerHTML = html;
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

function loadExample(query) {
    if (editor) {
        editor.setValue(query);
        editor.focus();
    }
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
            if (editor) {
                editor.setValue(`SELECT * FROM ${tableName};`);
                editor.focus();
            }
        });
    });
}

