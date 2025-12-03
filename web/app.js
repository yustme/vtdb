let editor;

// Initialize Monaco Editor
require.config({ paths: { vs: 'https://cdn.jsdelivr.net/npm/monaco-editor@0.45.0/min/vs' } });
require(['vs/editor/editor.main'], function () {
    editor = monaco.editor.create(document.getElementById('editor'), {
        value: '-- Enter your SQL query here\nSELECT * FROM users;',
        language: 'sql',
        theme: 'vs-dark',
        automaticLayout: true,
        minimap: { enabled: false },
        fontSize: 14,
        lineNumbers: 'on',
        roundedSelection: false,
        scrollBeyondLastLine: false,
    });
});

// Execute query
document.getElementById('execute-btn').addEventListener('click', executeQuery);

// Execute on Ctrl+Enter
document.addEventListener('keydown', function(e) {
    if (e.ctrlKey && e.key === 'Enter') {
        executeQuery();
    }
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

