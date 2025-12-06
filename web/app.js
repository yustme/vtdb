let editor;
let currentAbortController = null;
let executionTimerInterval = null;
let executionStartTime = null;
let progressPollInterval = null;
let currentQueryId = null;
let resultsDisplayed = false; // Flag to prevent timer from overwriting displayed results

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
    
    // Reset any previous execution time tracking to ensure clean start
    executionStartTime = null;
    resultsDisplayed = false; // Reset flag
    stopExecutionTimer();
    stopProgressPolling();
    
    // Create new abort controller for this query
    currentAbortController = new AbortController();
    
    // Update button to show cancel
    executeBtn.textContent = 'Cancel Query';
    executeBtn.classList.add('cancel-mode');
    
    // Set start time immediately when button is pressed (before any async operations)
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
        
        if (data.success) {
            // Check if this is an async query (has query_id)
            if (data.query_id) {
                currentQueryId = data.query_id;
                // Keep timer running for async queries - start polling for progress
                startProgressPolling(data.query_id);
            } else {
                // CRITICAL: Stop timer FIRST to prevent overwriting results
                stopExecutionTimer();
                
                // Extract EXACT values from backend response - NO client-side calculation
                // Backend response format: { execution_time_ms: 0.103666..., from_cache: true }
                const executionTime = (typeof data.execution_time_ms === 'number') ? data.execution_time_ms : 0;
                const fromCache = (data.from_cache === true);
                
                // Debug: Verify we're using backend values
                console.log('Backend response:', JSON.stringify({ execution_time_ms: data.execution_time_ms, from_cache: data.from_cache }));
                console.log('Using values:', { executionTime, fromCache });
                
                resetExecuteButton();
                if (data.result) {
                    // Pass ONLY backend-provided values - NO client-side timing
                    displayResults(data.result, executionTime, fromCache);
                } else {
                    // No results - show execution info
                    const cacheText = fromCache ? ' • Cache used' : '';
                    showSuccess(`Query executed successfully (no results) - Execution time: ${formatExecutionTime(executionTime)}${cacheText}`);
                }
            }
        } else {
            resetExecuteButton();
            showError(data.error || 'Unknown error occurred');
        }
    } catch (error) {
        stopExecutionTimer();
        stopProgressPolling();
        resetExecuteButton();
        
        if (error.name === 'AbortError') {
            // Cancel the query on the server if we have a query_id
            if (currentQueryId) {
                try {
                    await fetch(`/api/query/${currentQueryId}`, { method: 'DELETE' });
                } catch (e) {
                    // Ignore cancellation errors
                }
                currentQueryId = null;
            }
            resultsContainer.innerHTML = '<div class="info-message">Query execution cancelled by user</div>';
        } else {
            showError('Failed to execute query: ' + error.message);
        }
        // Only reset executionStartTime if query failed or was cancelled
        // For async queries, keep it until result is fetched
        if (!currentQueryId) {
            executionStartTime = null;
        }
    } finally {
        currentAbortController = null;
        // Don't reset executionStartTime here for async queries
        // It will be reset in fetchQueryResult() after displaying results
    }
}

async function cancelQuery() {
    // Cancel abort controller for fetch request
    if (currentAbortController) {
        currentAbortController.abort();
    }
    
    // Cancel query on server if we have a query_id
    if (currentQueryId) {
        try {
            await fetch(`/api/query/${currentQueryId}`, { method: 'DELETE' });
        } catch (err) {
            console.error('Failed to cancel query on server:', err);
        }
        currentQueryId = null;
    }
    
    // Stop timers and polling
    stopExecutionTimer();
    stopProgressPolling();
    
    // Reset execution start time
    executionStartTime = null;
    
    // Reset button
    resetExecuteButton();
    
    // Show cancellation message
    const resultsContainer = document.getElementById('results-container');
    resultsContainer.innerHTML = '<div class="info-message">Query execution cancelled by user</div>';
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
    // Also clear executionStartTime to prevent any timing calculations
    executionStartTime = null;
}

function startProgressPolling(queryId) {
    // Clear any existing polling
    stopProgressPolling();
    
    // Poll immediately
    pollProgress(queryId);
    
    // Poll every 5 seconds for progress updates
    progressPollInterval = setInterval(() => {
        pollProgress(queryId);
    }, 5000);
    
    // Keep the execution timer running to update elapsed time in progress display
    // The timer interval should already be running from startExecutionTimer()
    // But we'll ensure it updates the progress display
}

function stopProgressPolling() {
    if (progressPollInterval) {
        clearInterval(progressPollInterval);
        progressPollInterval = null;
    }
}

async function pollProgress(queryId) {
    try {
        const response = await fetch(`/api/query/${queryId}/progress`);
        const data = await response.json();
        
        if (data.success && data.progress) {
            displayProgress(data.progress);
            
            // Check if query is completed
            if (data.progress.status === 'Completed') {
                stopProgressPolling();
                stopExecutionTimer();
                // Fetch the result
                await fetchQueryResult(queryId);
            } else if (data.progress.status === 'Failed' || data.progress.status === 'Cancelled') {
                stopProgressPolling();
                stopExecutionTimer();
                resetExecuteButton();
                const errorMsg = data.progress.error_message || `Query ${data.progress.status.toLowerCase()}`;
                showError(errorMsg);
            }
        }
    } catch (error) {
        console.error('Failed to poll progress:', error);
    }
}

async function fetchQueryResult(queryId) {
    try {
        const response = await fetch(`/api/query/${queryId}/result`);
        const data = await response.json();
        
        if (data.success && data.result) {
            // CRITICAL: Stop timer FIRST to prevent overwriting results
            stopExecutionTimer();
            
            // Extract EXACT values from backend response
            // Backend response: { execution_time_ms: 0.103666..., from_cache: true }
            const executionTime = (typeof data.execution_time_ms === 'number') ? data.execution_time_ms : 0;
            const fromCache = (data.from_cache === true);
            
            // Debug logging to verify we're using backend values
            console.log('Using backend values (async):', {
                'data.execution_time_ms': data.execution_time_ms,
                'data.from_cache': data.from_cache,
                'extracted executionTime (ms)': executionTime,
                'extracted fromCache': fromCache
            });
            
            resetExecuteButton();
            // Pass ONLY backend-provided values - NO client-side calculation
            displayResults(data.result, executionTime, fromCache);
        } else {
            stopExecutionTimer();
            showError(data.error || 'Failed to fetch query result');
        }
    } catch (error) {
        stopExecutionTimer();
        showError('Failed to fetch query result: ' + error.message);
    } finally {
        currentQueryId = null;
        // Reset executionStartTime after result is displayed
        executionStartTime = null;
    }
}

function updateProgressTimer() {
    // Update elapsed time in progress display if it exists
    if (!currentQueryId || !executionStartTime) return;
    
    const resultsContainer = document.getElementById('results-container');
    const progressDisplay = resultsContainer.querySelector('.progress-display');
    if (progressDisplay) {
        const elapsed = performance.now() - executionStartTime;
        const elapsedFormatted = formatExecutionTime(elapsed);
        const statusDiv = progressDisplay.querySelector('.progress-status');
        if (statusDiv) {
            // Extract status from current progress or use Running
            const statusText = statusDiv.textContent.match(/Status:\s*<strong>([^<]+)<\/strong>/);
            const status = statusText ? statusText[1] : 'Running';
            statusDiv.innerHTML = `Status: <strong>${status}</strong> • Running for: ${elapsedFormatted}`;
        }
    }
}

function displayProgress(progress) {
    const resultsContainer = document.getElementById('results-container');
    
    // Calculate elapsed time for display
    // Defensive check: ensure executionStartTime is valid before calculating
    const elapsed = executionStartTime ? (performance.now() - executionStartTime) : 0;
    const elapsedFormatted = formatExecutionTime(elapsed);
    
    let progressHtml = '<div class="progress-display">';
    progressHtml += `<div class="progress-header">`;
    progressHtml += `<h3>Query Progress</h3>`;
    progressHtml += `<div class="progress-status">Status: <strong>${progress.status}</strong> • Running for: ${elapsedFormatted}</div>`;
    progressHtml += `</div>`;
    
    progressHtml += `<div class="progress-stage">Current Stage: ${progress.current_stage}</div>`;
    
    // Add cancel button
    progressHtml += `<div style="margin-top: 15px;">`;
    progressHtml += `<button class="cancel-query-btn" id="progress-cancel-btn">Cancel Query</button>`;
    progressHtml += `</div>`;
    
    // Overall progress
    const progressPercent = progress.estimated_total_rows > 0 
        ? Math.round((progress.rows_processed / progress.estimated_total_rows) * 100)
        : 0;
    progressHtml += `<div class="progress-bar-container">`;
    progressHtml += `<div class="progress-bar" style="width: ${progressPercent}%"></div>`;
    progressHtml += `</div>`;
    progressHtml += `<div class="progress-info">Rows processed: ${progress.rows_processed.toLocaleString()} / ${progress.estimated_total_rows.toLocaleString()} (${progressPercent}%)</div>`;
    
    // Table progress
    if (progress.tables_scanned && progress.tables_scanned.length > 0) {
        progressHtml += `<div class="table-progress-list">`;
        progress.tables_scanned.forEach(table => {
            const tablePercent = table.total_rows > 0 
                ? Math.round((table.rows_scanned / table.total_rows) * 100)
                : 0;
            progressHtml += `<div class="table-progress-item">`;
            progressHtml += `<div class="table-progress-name">${table.table_name}</div>`;
            progressHtml += `<div class="table-progress-bar-container">`;
            progressHtml += `<div class="table-progress-bar" style="width: ${tablePercent}%"></div>`;
            progressHtml += `</div>`;
            progressHtml += `<div class="table-progress-info">${table.rows_scanned.toLocaleString()} / ${table.total_rows.toLocaleString()} rows (${tablePercent}%)</div>`;
            progressHtml += `</div>`;
        });
        progressHtml += `</div>`;
    }
    
    // Estimated time remaining
    if (progress.rows_processed > 0 && progress.estimated_total_rows > 0) {
        const elapsed = progress.elapsed_seconds || 0;
        const rate = progress.rows_processed / elapsed; // rows per second
        if (rate > 0) {
            const remaining = progress.estimated_total_rows - progress.rows_processed;
            const estimatedSeconds = remaining / rate;
            progressHtml += `<div class="progress-estimate">Estimated remaining: ~${formatTimeEstimate(estimatedSeconds)}</div>`;
        }
    }
    
    progressHtml += `</div>`;
    
    resultsContainer.innerHTML = progressHtml;
    
    // Add event listener to cancel button
    const cancelBtn = document.getElementById('progress-cancel-btn');
    if (cancelBtn) {
        cancelBtn.addEventListener('click', cancelQuery);
    }
}

function formatTimeEstimate(seconds) {
    if (seconds < 60) {
        return `${Math.round(seconds)}s`;
    } else if (seconds < 3600) {
        return `${Math.round(seconds / 60)}m ${Math.round(seconds % 60)}s`;
    } else {
        const hours = Math.floor(seconds / 3600);
        const minutes = Math.floor((seconds % 3600) / 60);
        return `${hours}h ${minutes}m`;
    }
}

function updateExecutionTimer() {
    if (!executionStartTime) return;
    
    // CRITICAL: Don't update if results have already been displayed
    // Check flag first, then check DOM
    if (resultsDisplayed) {
        stopExecutionTimer();
        return;
    }
    
    const resultsContainer = document.getElementById('results-container');
    if (resultsContainer) {
        const hasResults = resultsContainer.querySelector('.results-table');
        const hasExecutionTime = Array.from(resultsContainer.querySelectorAll('.info-message')).some(
            msg => msg.textContent.includes('EXECUTION TIME')
        );
        if (hasResults || hasExecutionTime) {
            // Results already displayed with backend-provided execution time
            // Stop timer to prevent overwriting
            resultsDisplayed = true;
            stopExecutionTimer();
            return;
        }
    }
    
    const elapsed = performance.now() - executionStartTime;
    const elapsedFormatted = formatExecutionTime(elapsed);
    
    // If we have an async query, update the progress display timer
    if (currentQueryId) {
        updateProgressTimer();
        return;
    }
    
    // For synchronous queries, update the loading display
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
    
    // CRITICAL: executionTime MUST be the backend-provided execution_time_ms value in milliseconds
    // fromCache MUST be the backend-provided from_cache boolean value
    // These values come directly from the API response, NOT from client-side timing
    
    // Set flag and stop timer to prevent overwriting
    resultsDisplayed = true;
    stopExecutionTimer();
    
    // Debug: Log what we're displaying
    console.log('displayResults - displaying backend values:', {
        'executionTime (ms from backend)': executionTime,
        'fromCache (from backend)': fromCache,
        'formatted time': formatExecutionTime(executionTime)
    });
    
    if (!result.columns || result.columns.length === 0) {
        const cacheText = fromCache ? ' • Cache used' : '';
        resultsContainer.innerHTML = `
            <div class="execution-info-widget">
                <span class="execution-time">⏱️ ${formatExecutionTime(executionTime)}</span>
                ${fromCache ? '<span class="cache-badge">Cache used</span>' : ''}
            </div>
            <div class="info-message">Query executed successfully (no columns)</div>`;
        return;
    }
    
    if (!result.rows || result.rows.length === 0) {
        const cacheText = fromCache ? ' • Cache used' : '';
        resultsContainer.innerHTML = `
            <div class="execution-info-widget">
                <span class="execution-time">⏱️ ${formatExecutionTime(executionTime)}</span>
                ${fromCache ? '<span class="cache-badge">Cache used</span>' : ''}
            </div>
            <div class="info-message">Query executed successfully (0 rows returned)</div>`;
        return;
    }
    
    const totalRows = result.rows.length;
    const rowsToDisplay = result.rows.slice(0, MAX_DISPLAY_ROWS);
    const isTruncated = totalRows > MAX_DISPLAY_ROWS;
    
    let html = '';
    
    // Show execution time and cache status widget
    html += `<div class="execution-info-widget">
        <span class="execution-time">⏱️ ${formatExecutionTime(executionTime)}</span>
        ${fromCache ? '<span class="cache-badge">Cache used</span>' : ''}
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

