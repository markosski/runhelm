import { useState } from 'react';
import './index.css';

// Simple SVG Icons
const IconDashboard = () => (
  <svg xmlns="http://www.开展w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><rect x="3" y="3" width="7" height="9" rx="1"/><rect x="14" y="3" width="7" height="5" rx="1"/><rect x="14" y="12" width="7" height="9" rx="1"/><rect x="3" y="16" width="7" height="5" rx="1"/></svg>
);

const IconWorkflow = () => (
  <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"/><rect x="8" y="2" width="8" height="4" rx="1" ry="1"/><path d="M12 11h4"/><path d="M12 16h4"/><path d="M8 11h.01"/><path d="M8 16h.01"/></svg>
);

const IconRuns = () => (
  <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><polygon points="5 3 19 12 5 21 5 3"/></svg>
);

const IconSettings = () => (
  <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>
);

const IconPlus = () => (
  <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
);

const IconActivity = () => (
  <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><polyline points="22 12 18 12 15 21 9 3 6 12 2 12"/></svg>
);

const IconCheckCircle = () => (
  <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/></svg>
);

function App() {
  const [activeTab, setActiveTab] = useState('dashboard');

  return (
    <div className="app-container">
      {/* Sidebar */}
      <aside className="sidebar">
        <div className="sidebar-logo">
          <svg xmlns="http://www.w3.org/2000/svg" width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><polygon points="12 2 2 7 12 12 22 7 12 2"/><polyline points="2 17 12 22 22 17"/><polyline points="2 12 12 17 22 12"/></svg>
          RunHelm
        </div>

        <div className="nav-section">
          <div className="nav-section-title">Overview</div>
          <ul className="nav-menu">
            <li className={`nav-item ${activeTab === 'dashboard' ? 'active' : ''}`} onClick={() => setActiveTab('dashboard')}>
              <IconDashboard /> Dashboard
            </li>
            <li className={`nav-item ${activeTab === 'workflows' ? 'active' : ''}`} onClick={() => setActiveTab('workflows')}>
              <IconWorkflow /> Workflows
            </li>
            <li className={`nav-item ${activeTab === 'runs' ? 'active' : ''}`} onClick={() => setActiveTab('runs')}>
              <IconRuns /> Runs
            </li>
          </ul>
        </div>

        <div className="nav-section">
          <div className="nav-section-title">System</div>
          <ul className="nav-menu">
            <li className={`nav-item ${activeTab === 'settings' ? 'active' : ''}`} onClick={() => setActiveTab('settings')}>
              <IconSettings /> Settings
            </li>
          </ul>
        </div>
      </aside>

      {/* Main Content */}
      <main className="main-content">
        <header className="page-header animate-fade-in">
          <div>
            <h1 className="page-title">Dashboard</h1>
            <p className="page-subtitle">Welcome back to RunHelm Orchestrator</p>
          </div>
          <button className="btn btn-primary">
            <IconPlus /> New Workflow
          </button>
        </header>

        {/* Dashboard Grid */}
        <div className="dashboard-grid animate-fade-in delay-1">
          <div className="stat-card glass-panel">
            <div className="stat-header">
              <span className="stat-title">Active Runs</span>
              <div className="stat-icon"><IconActivity /></div>
            </div>
            <div className="stat-value">12</div>
          </div>
          
          <div className="stat-card glass-panel">
            <div className="stat-header">
              <span className="stat-title">Completed Tasks</span>
              <div className="stat-icon" style={{ color: 'var(--color-accent)', background: 'rgba(16, 185, 129, 0.1)' }}>
                <IconCheckCircle />
              </div>
            </div>
            <div className="stat-value">8,241</div>
          </div>
          
          <div className="stat-card glass-panel">
            <div className="stat-header">
              <span className="stat-title">Failed Tasks</span>
              <div className="stat-icon" style={{ color: '#ef4444', background: 'rgba(239, 68, 68, 0.1)' }}>
                <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="10"/><line x1="15" y1="9" x2="9" y2="15"/><line x1="9" y1="9" x2="15" y2="15"/></svg>
              </div>
            </div>
            <div className="stat-value">3</div>
          </div>
        </div>

        {/* Recent Activity */}
        <div className="animate-fade-in delay-2">
          <h2 className="section-title">Recent Runs</h2>
          <div className="activity-list glass-panel" style={{ padding: '8px' }}>
            
            <div className="activity-item">
              <div className="activity-info">
                <div className="activity-icon">
                  <IconWorkflow />
                </div>
                <div className="activity-details">
                  <div className="activity-name">Data Pipeline Etl</div>
                  <div className="activity-meta">
                    Started 2 mins ago • Triggered by schedule
                  </div>
                </div>
              </div>
              <div className="badge badge-running">Running</div>
            </div>

            <div className="activity-item">
              <div className="activity-info">
                <div className="activity-icon">
                  <IconWorkflow />
                </div>
                <div className="activity-details">
                  <div className="activity-name">Daily Report Generation</div>
                  <div className="activity-meta">
                    Completed 1 hour ago • 45 tasks executed
                  </div>
                </div>
              </div>
              <div className="badge badge-success">Completed</div>
            </div>

            <div className="activity-item">
              <div className="activity-info">
                <div className="activity-icon">
                  <IconWorkflow />
                </div>
                <div className="activity-details">
                  <div className="activity-name">Slack Notification Agent</div>
                  <div className="activity-meta">
                    Failed 3 hours ago • Error in API integration
                  </div>
                </div>
              </div>
              <div className="badge badge-failed">Failed</div>
            </div>

          </div>
        </div>
      </main>
    </div>
  );
}

export default App;
