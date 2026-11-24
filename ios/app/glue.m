// The iOS side of src/ios.rs: the connection extension through NETunnelProviderManager, the clipboard,
// links and the phone's language. States go back to Rust through nc_event, as Java's nativeEvent does
// on Android: 0 off, 1 connecting, 2 connected (text: ms), 3 error (text: message), 4 no answer.
#import <Foundation/Foundation.h>
#import <NetworkExtension/NetworkExtension.h>
#import <UIKit/UIKit.h>
#include <stdlib.h>
#include <string.h>

extern void nc_event(int kind, const char *text);

static NSString *const kProvider = @"com.appomobi.drawnnetc.connection";
static NETunnelProviderManager *manager;
static id observer;
static NEVPNStatus lastStatus = NEVPNStatusInvalid;
static BOOL stopping;

static void emit(int kind, NSString *text) { nc_event(kind, (text ?: @"").UTF8String); }

static char *copied(NSString *text) { return strdup((text ?: @"").UTF8String); }

void nc_free(char *text) { free(text); }

/// A test request through the server, in the extension: connected with its delay, or no answer.
void nc_test(void) {
    NETunnelProviderSession *session = (NETunnelProviderSession *)manager.connection;
    if (session == nil || session.status != NEVPNStatusConnected) return;
    NSError *error = nil;
    BOOL sent = [session sendProviderMessage:[@"test" dataUsingEncoding:NSUTF8StringEncoding]
                                 returnError:&error
                             responseHandler:^(NSData *data) {
        NSString *ms = data ? [[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding] : @"";
        if (ms.length > 0) emit(2, ms);
        else emit(4, @"The server does not answer");
    }];
    if (!sent) emit(4, error.localizedDescription);
}

static void statusChanged(void) {
    NEVPNConnection *connection = manager.connection;
    NEVPNStatus status = connection ? connection.status : NEVPNStatusInvalid;
    NEVPNStatus before = lastStatus;
    lastStatus = status;
    switch (status) {
        case NEVPNStatusConnecting:
        case NEVPNStatusReasserting:
            emit(1, @"Connecting");
            break;
        case NEVPNStatusConnected:
            emit(1, @"Connecting");
            nc_test();
            break;
        case NEVPNStatusDisconnecting:
            break;
        case NEVPNStatusInvalid:
        case NEVPNStatusDisconnected:
        default: {
            BOOL failed = !stopping && (before == NEVPNStatusConnecting || before == NEVPNStatusReasserting);
            stopping = NO;
            if (failed) {
                [connection fetchLastDisconnectErrorWithCompletionHandler:^(NSError *error) {
                    emit(3, error.localizedDescription ?: @"Could not connect");
                }];
            } else {
                emit(0, @"");
            }
            break;
        }
    }
}

static void watch(NETunnelProviderManager *found) {
    if (observer) [[NSNotificationCenter defaultCenter] removeObserver:observer];
    manager = found;
    observer = [[NSNotificationCenter defaultCenter] addObserverForName:NEVPNStatusDidChangeNotification
                                                                 object:found.connection
                                                                  queue:[NSOperationQueue mainQueue]
                                                             usingBlock:^(NSNotification *note) { statusChanged(); }];
}

/// Our configuration among the saved ones (nil before the first connect).
static void load(void (^done)(NETunnelProviderManager *found, NSError *error)) {
    [NETunnelProviderManager loadAllFromPreferencesWithCompletionHandler:^(NSArray<NETunnelProviderManager *> *all, NSError *error) {
        NETunnelProviderManager *found = nil;
        for (NETunnelProviderManager *m in all) {
            NETunnelProviderProtocol *p = (NETunnelProviderProtocol *)m.protocolConfiguration;
            if ([p isKindOfClass:[NETunnelProviderProtocol class]] && [p.providerBundleIdentifier isEqualToString:kProvider]) {
                found = m;
                break;
            }
        }
        dispatch_async(dispatch_get_main_queue(), ^{ done(found, error); });
    }];
}

/// At start: the state of a connection that is already up (or not).
void nc_start(void) {
    load(^(NETunnelProviderManager *found, NSError *error) {
        if (found) {
            watch(found);
            statusChanged();
        }
    });
}

void nc_connect(const char *config_utf8) {
    NSString *config = [NSString stringWithUTF8String:config_utf8];
    emit(1, @"Connecting");
    load(^(NETunnelProviderManager *found, NSError *loadError) {
        NETunnelProviderManager *m = found ?: [NETunnelProviderManager new];
        NETunnelProviderProtocol *p = [NETunnelProviderProtocol new];
        p.providerBundleIdentifier = kProvider;
        p.serverAddress = @"Net Connect";
        // Kept for the system's own restarts of the tunnel.
        p.providerConfiguration = @{@"config": config};
        m.protocolConfiguration = p;
        m.localizedDescription = @"Net Connect";
        m.enabled = YES;
        // The first time, the system asks the user to allow the configuration.
        [m saveToPreferencesWithCompletionHandler:^(NSError *saveError) {
            if (saveError) {
                // Don't Allow (or no passcode): "permission", for which the app shows its own text,
                // as Android's MainActivity sends it.
                BOOL refused = [saveError.domain isEqualToString:NEVPNErrorDomain] && saveError.code == NEVPNErrorConfigurationReadWriteFailed;
                NSString *detail = refused ? @"permission" : saveError.localizedDescription;
                dispatch_async(dispatch_get_main_queue(), ^{ emit(3, detail); });
                return;
            }
            // A saved configuration must be loaded again before it can start.
            [m loadFromPreferencesWithCompletionHandler:^(NSError *reloadError) {
                dispatch_async(dispatch_get_main_queue(), ^{
                    watch(m);
                    stopping = NO;
                    NSError *startError = nil;
                    [(NETunnelProviderSession *)m.connection startTunnelWithOptions:@{@"config": config} andReturnError:&startError];
                    if (startError) emit(3, startError.localizedDescription);
                });
            }];
        }];
    });
}

void nc_disconnect(void) {
    dispatch_async(dispatch_get_main_queue(), ^{
        if (manager == nil) {
            emit(0, @"");
            return;
        }
        stopping = YES;
        [manager.connection stopVPNTunnel];
    });
}

void nc_copy_text(const char *text) {
    NSString *s = [NSString stringWithUTF8String:text];
    dispatch_async(dispatch_get_main_queue(), ^{ UIPasteboard.generalPasteboard.string = s; });
}

char *nc_clipboard_text(void) {
    __block NSString *text = nil;
    void (^read)(void) = ^{ text = UIPasteboard.generalPasteboard.string; };
    if (NSThread.isMainThread) read(); else dispatch_sync(dispatch_get_main_queue(), read);
    return copied(text);
}

void nc_open_url(const char *url) {
    NSURL *u = [NSURL URLWithString:[NSString stringWithUTF8String:url]];
    if (u == nil) return;
    dispatch_async(dispatch_get_main_queue(), ^{ [UIApplication.sharedApplication openURL:u options:@{} completionHandler:nil]; });
}

/// "ru", "en", ...: the phone's first preferred language.
char *nc_system_language(void) {
    NSString *first = NSLocale.preferredLanguages.firstObject ?: @"";
    return copied([first componentsSeparatedByString:@"-"].firstObject);
}

/// The engine's version, its number written into Info.plist at build (ios/ios.sh): "Xray-core v26.11.9".
char *nc_core_version(void) {
    NSString *number = [NSBundle.mainBundle objectForInfoDictionaryKey:@"EngineVersion"];
    return copied(number.length ? [@"Xray-core v" stringByAppendingString:number] : @"");
}
