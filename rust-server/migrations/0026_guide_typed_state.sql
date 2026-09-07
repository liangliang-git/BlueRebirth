CREATE TABLE IF NOT EXISTS guide_settings (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    setting_key TEXT NOT NULL,
    setting_value TEXT NOT NULL,
    PRIMARY KEY(profile_id, setting_key)
);

CREATE TABLE IF NOT EXISTS guide_plot_rewards (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    plot_id INTEGER NOT NULL CHECK(plot_id > 0),
    PRIMARY KEY(profile_id, plot_id)
);
