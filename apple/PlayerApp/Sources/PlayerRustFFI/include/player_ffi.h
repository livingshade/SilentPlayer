#ifndef PLAYER_FFI_H
#define PLAYER_FFI_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

typedef struct PlayerApp PlayerApp;

PlayerApp *player_app_create(const char *db_path, const char *media_root);
void player_app_destroy(PlayerApp *app);
void player_string_free(char *value);

char *player_app_export_library(PlayerApp *app, const char *package_path);
char *player_app_import_library(PlayerApp *app, const char *package_path);
char *player_app_zero_out_library(PlayerApp *app);

char *player_app_import_folder(PlayerApp *app, const char *folder);
char *player_app_import_files(PlayerApp *app, const char *paths_json);
char *player_app_library(PlayerApp *app);
char *player_app_library_page(PlayerApp *app, size_t offset, size_t limit);
char *player_app_search(PlayerApp *app, const char *query, size_t limit);
char *player_app_analyze(PlayerApp *app);
char *player_app_audit_database(PlayerApp *app);
char *player_app_user_data(PlayerApp *app);

char *player_app_play_library(PlayerApp *app);
char *player_app_play_path(PlayerApp *app, const char *path);
char *player_app_play_queue(PlayerApp *app, const char *paths_json, const char *start_path);
char *player_app_play_playlist(PlayerApp *app, const char *name, const char *start_path, bool shuffle);
char *player_app_pause(PlayerApp *app);
char *player_app_resume(PlayerApp *app);
char *player_app_audio_interruption_began(PlayerApp *app);
char *player_app_audio_interruption_ended(PlayerApp *app, bool system_should_resume);
char *player_app_audio_output_disconnected(PlayerApp *app);
char *player_app_stop(PlayerApp *app);
char *player_app_next(PlayerApp *app);
char *player_app_previous(PlayerApp *app);
char *player_app_seek(PlayerApp *app, uint64_t position_ms);
char *player_app_poll(PlayerApp *app);
char *player_app_set_repeat_mode(PlayerApp *app, const char *repeat_mode);
char *player_app_set_shuffle(PlayerApp *app, bool enabled);
char *player_app_queue(PlayerApp *app);
char *player_app_track_details(PlayerApp *app, const char *path);
char *player_app_edit_track_view(PlayerApp *app, const char *path, const char *edit_json);
char *player_app_set_track_notes(PlayerApp *app, const char *path, const char *notes);
char *player_app_set_track_rating(PlayerApp *app, const char *path, int32_t rating);
char *player_app_set_track_metadata(PlayerApp *app, const char *path, const char *title, const char *artist, const char *album);
char *player_app_set_track_artwork(PlayerApp *app, const char *path, const char *image_path);
char *player_app_set_album_artwork(PlayerApp *app, const char *path, const char *image_path);
char *player_app_set_track_lyrics(PlayerApp *app, const char *path, const char *lyrics_path);
char *player_app_export_track_view(PlayerApp *app, const char *path, const char *destination);

char *player_app_set_favorite(PlayerApp *app, const char *path, bool enabled);
char *player_app_favorites(PlayerApp *app);
char *player_app_history(PlayerApp *app, size_t limit);

char *player_app_playlists(PlayerApp *app);
char *player_app_create_playlist(PlayerApp *app, const char *name);
char *player_app_rename_playlist(PlayerApp *app, const char *old_name, const char *new_name);
char *player_app_set_playlist_artwork(PlayerApp *app, const char *name, const char *image_path);
char *player_app_delete_playlist(PlayerApp *app, const char *name);
char *player_app_clear_playlist(PlayerApp *app, const char *name);
char *player_app_add_to_playlist(PlayerApp *app, const char *name, const char *path);
char *player_app_remove_from_playlist(PlayerApp *app, const char *name, const char *path);
char *player_app_move_playlist_track(PlayerApp *app, const char *name, const char *path, int32_t delta);
char *player_app_sort_playlist(PlayerApp *app, const char *name, const char *sort);
char *player_app_playlist_tracks(PlayerApp *app, const char *name);

#endif
