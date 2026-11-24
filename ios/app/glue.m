// The system side of src/apple.rs, iOS and macOS: the connection extension through
// NETunnelProviderManager (on macOS a system extension, activated first), the clipboard, links and
// the language; on macOS also the update check's download. States go back to Rust through nc_event,
// as Java's nativeEvent does on Android: 0 off, 1 connecting, 2 connected (text: ms), 3 error
// (text: message), 4 no answer, 11 a fetched text.
#import <Foundation/Foundation.h>
#import <NetworkExtension/NetworkExtension.h>
#include <TargetConditionals.h>
#if TARGET_OS_OSX
#import <AppKit/AppKit.h>
#import <SystemExtensions/SystemExtensions.h>
#else
#import <UIKit/UIKit.h>
#endif
#include <stdlib.h>
#include <string.h>

extern void nc_event(int kind, const char *text);

static NSString *const kProvider = @"com.appomobi.drawnnetc.connection";
static NETunnelProviderManager *manager;
static id observer;
static NEVPNStatus lastStatus = NEVPNStatusInvalid;
static BOOL stopping;
/// A configuration to start once the running tunnel is down (a switch of server while connected).
static NSString *pendingConfig;

static void emit(int kind, NSString *text) { nc_event(kind, (text ?: @"").UTF8String); }

static char *copied(NSString *text) { return strdup((text ?: @"").UTF8String); }

void nc_free(char *text) { free(text); }

#if TARGET_OS_OSX
/// macOS: the tunnel is a system extension inside the app, which the system installs on request
/// (asking the user once in System Settings) before any configuration can use it.
/// The extension waits for the user to turn it on (System Settings).
static BOOL approvalPending;

/// Where the user turns the extension on: the Network Extensions sheet of General > Login Items &
/// Extensions, opened directly with its switch for Net Connect; and the app's own line saying so
/// ("extension").
static void askForApproval(void) {
    approvalPending = YES;
    emit(3, @"extension");
    [NSWorkspace.sharedWorkspace openURL:[NSURL URLWithString:
        @"x-apple.systempreferences:com.apple.ExtensionsPreferences?extensionPointIdentifier=com.apple.system_extension.network_extension.extension-point"]];
}

@interface NCExtensionActivator : NSObject <OSSystemExtensionRequestDelegate>
@property (copy) void (^done)(NSError *error);
@end

@implementation NCExtensionActivator
- (OSSystemExtensionReplacementAction)request:(OSSystemExtensionRequest *)request
                  actionForReplacingExtension:(OSSystemExtensionProperties *)existing
                                withExtension:(OSSystemExtensionProperties *)extension {
    return OSSystemExtensionReplacementActionReplace;
}
- (void)requestNeedsUserApproval:(OSSystemExtensionRequest *)request {
    // The request goes on by itself once the user turns the extension on.
    dispatch_async(dispatch_get_main_queue(), ^{ askForApproval(); });
}
- (void)request:(OSSystemExtensionRequest *)request didFinishWithResult:(OSSystemExtensionRequestResult)result {
    if (result == OSSystemExtensionRequestWillCompleteAfterReboot) {
        self.done([NSError errorWithDomain:@"connection" code:2 userInfo:@{NSLocalizedDescriptionKey: @"Restart the Mac to finish installing"}]);
    } else {
        self.done(nil);
    }
}
- (void)request:(OSSystemExtensionRequest *)request didFailWithError:(NSError *)error {
    self.done(error);
}
@end

static NCExtensionActivator *activator;

/// Installs (or updates) the extension, then runs `then` on the main queue.
static void activateExtension(void (^then)(void)) {
    // Still off since an earlier press (the user closed the system's alert with OK): every press
    // of Connect opens the settings page again, not only the first.
    if (approvalPending) askForApproval();
    activator = [NCExtensionActivator new];
    activator.done = ^(NSError *error) {
        dispatch_async(dispatch_get_main_queue(), ^{
            approvalPending = NO;
            if (error) {
                emit(3, error.localizedDescription);
            } else {
                emit(1, @"Connecting");
                then();
            }
        });
    };
    OSSystemExtensionRequest *request = [OSSystemExtensionRequest activationRequestForExtension:@"com.appomobi.drawnnetc.connection"
                                                                                         queue:dispatch_get_main_queue()];
    request.delegate = activator;
    [OSSystemExtensionManager.sharedManager submitRequest:request];
}
#else
static void activateExtension(void (^then)(void)) { then(); }
#endif

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

static void startTunnel(NETunnelProviderManager *m, NSString *config) {
    stopping = NO;
    NSError *startError = nil;
    [(NETunnelProviderSession *)m.connection startTunnelWithOptions:@{@"config": config} andReturnError:&startError];
    if (startError) emit(3, startError.localizedDescription);
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
            if (pendingConfig) {
                // The old tunnel is down: the new server's starts now.
                NSString *config = pendingConfig;
                pendingConfig = nil;
                emit(1, @"Connecting");
                startTunnel(manager, config);
                break;
            }
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
        } else {
            // No configuration yet (the first start): off. The app's first state event also
            // fills its texts.
            emit(0, @"");
        }
    });
}

void nc_connect(const char *config_utf8) {
    NSString *config = [NSString stringWithUTF8String:config_utf8];
    emit(1, @"Connecting");
    activateExtension(^{ load(^(NETunnelProviderManager *found, NSError *loadError) {
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
                    NEVPNStatus status = m.connection.status;
                    if (status == NEVPNStatusConnected || status == NEVPNStatusConnecting ||
                        status == NEVPNStatusReasserting || status == NEVPNStatusDisconnecting) {
                        // Another server is up (a switch): a running tunnel ignores a start, so it
                        // stops first and the new one starts once it is down (statusChanged).
                        pendingConfig = config;
                        stopping = YES;
                        if (status != NEVPNStatusDisconnecting) [m.connection stopVPNTunnel];
                    } else {
                        startTunnel(m, config);
                    }
                });
            }];
        }];
    }); });
}

void nc_disconnect(void) {
    dispatch_async(dispatch_get_main_queue(), ^{
        if (manager == nil) {
            emit(0, @"");
            return;
        }
        stopping = YES;
        pendingConfig = nil;
        [manager.connection stopVPNTunnel];
    });
}

void nc_copy_text(const char *text) {
    NSString *s = [NSString stringWithUTF8String:text];
#if TARGET_OS_OSX
    dispatch_async(dispatch_get_main_queue(), ^{
        [NSPasteboard.generalPasteboard clearContents];
        [NSPasteboard.generalPasteboard setString:s forType:NSPasteboardTypeString];
    });
#else
    dispatch_async(dispatch_get_main_queue(), ^{ UIPasteboard.generalPasteboard.string = s; });
#endif
}

char *nc_clipboard_text(void) {
    __block NSString *text = nil;
#if TARGET_OS_OSX
    void (^read)(void) = ^{ text = [NSPasteboard.generalPasteboard stringForType:NSPasteboardTypeString]; };
#else
    void (^read)(void) = ^{ text = UIPasteboard.generalPasteboard.string; };
#endif
    if (NSThread.isMainThread) read(); else dispatch_sync(dispatch_get_main_queue(), read);
    return copied(text);
}

void nc_open_url(const char *url) {
    NSURL *u = [NSURL URLWithString:[NSString stringWithUTF8String:url]];
    if (u == nil) return;
#if TARGET_OS_OSX
    dispatch_async(dispatch_get_main_queue(), ^{ [NSWorkspace.sharedWorkspace openURL:u]; });
#else
    dispatch_async(dispatch_get_main_queue(), ^{ [UIApplication.sharedApplication openURL:u options:@{} completionHandler:nil]; });
#endif
}

#if TARGET_OS_OSX
/// The update check's download (macOS): the text, or "" when it failed, as event 11, with the
/// headers Android's fetchText sends.
void nc_fetch(const char *url_utf8) {
    NSURL *url = [NSURL URLWithString:[NSString stringWithUTF8String:url_utf8]];
    if (url == nil) { emit(11, @""); return; }
    NSMutableURLRequest *request = [NSMutableURLRequest requestWithURL:url cachePolicy:NSURLRequestReloadIgnoringLocalCacheData timeoutInterval:20];
    [request setValue:@"NetConnect" forHTTPHeaderField:@"User-Agent"];
    [request setValue:@"application/vnd.github+json" forHTTPHeaderField:@"Accept"];
    [[NSURLSession.sharedSession dataTaskWithRequest:request completionHandler:^(NSData *data, NSURLResponse *response, NSError *error) {
        NSInteger code = [response isKindOfClass:NSHTTPURLResponse.class] ? ((NSHTTPURLResponse *)response).statusCode : 0;
        NSString *body = (code == 200 && data) ? [[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding] : @"";
        emit(11, body ?: @"");
    }] resume];
}
#endif

/// "ru", "en", ...: the first preferred language.
char *nc_system_language(void) {
    NSString *first = NSLocale.preferredLanguages.firstObject ?: @"";
    return copied([first componentsSeparatedByString:@"-"].firstObject);
}

/// The engine's version, its number written into Info.plist at build (ios/ios.sh): "Xray-core v26.11.9".
char *nc_core_version(void) {
    NSString *number = [NSBundle.mainBundle objectForInfoDictionaryKey:@"EngineVersion"];
    return copied(number.length ? [@"Xray-core v" stringByAppendingString:number] : @"");
}
