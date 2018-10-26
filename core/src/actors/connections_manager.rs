use futures::Stream;
use log::{debug, info};
use std::net::SocketAddr;

use actix::actors::resolver::{ConnectAddr, Resolver, ResolverError};
use actix::fut::FutureResult;
use actix::io::FramedWrite;
use actix::{
    Actor, ActorFuture, AsyncContext, Context, ContextFutureSpawner, Handler, MailboxError,
    Message, StreamHandler, SystemService, WrapFuture,
};
use tokio::codec::FramedRead;
use tokio::io::AsyncRead;
use tokio::net::{TcpListener, TcpStream};

use crate::actors::codec::P2PCodec;
use crate::actors::session::Session;

use witnet_p2p::sessions::SessionType;

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR MESSAGES
////////////////////////////////////////////////////////////////////////////////////////
/// Actor message that holds the TCP stream from an inbound TCP connection
#[derive(Message)]
struct InboundTcpConnect {
    stream: TcpStream,
}

impl InboundTcpConnect {
    /// Method to create a new InboundTcpConnect message from a TCP stream
    fn new(stream: TcpStream) -> InboundTcpConnect {
        InboundTcpConnect { stream }
    }
}

/// Actor message to request the creation of an outbound TCP connection to a peer.
/// The address of the peer is not specified as it will be determined by the peers manager actor.
#[derive(Message)]
pub struct OutboundTcpConnect {
    /// Address of the outbound connection
    pub address: SocketAddr,
}

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR BASIC STRUCTURE
////////////////////////////////////////////////////////////////////////////////////////
/// Connections manager actor
#[derive(Default)]
pub struct ConnectionsManager;

/// Make actor from `ConnectionsManager`
impl Actor for ConnectionsManager {
    /// Every actor has to provide execution `Context` in which it can run.
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        debug!("Connections Manager actor has been started!");

        // Start server
        // TODO[23-10-2018]: handle errors when starting server appropiately
        ConnectionsManager::start_server(ctx);
    }
}

/// Required trait for being able to retrieve connections manager address from system registry
impl actix::Supervised for ConnectionsManager {}

/// Required trait for being able to retrieve connections manager address from system registry
impl SystemService for ConnectionsManager {}

/// Auxiliary methods for `ConnectionsManager` actor
impl ConnectionsManager {
    /// Method to start a server
    fn start_server(ctx: &mut <Self as Actor>::Context) {
        debug!("Trying to start P2P server...");

        // Get address to launch the server
        // TODO[23-10-2018]: query server address from config manager
        let server_address = "127.0.0.1:50000".parse().unwrap();

        // Bind TCP listener to this address
        // TODO[23-10-2018]: handle errors
        let listener = TcpListener::bind(&server_address).unwrap();

        // Add message stream which will return a InboundTcpConnect for each incoming TCP connection
        ctx.add_message_stream(
            listener
                .incoming()
                .map_err(|_| ())
                .map(InboundTcpConnect::new),
        );

        info!("P2P server has been started at {:?}", server_address);
    }

    /// Method to create a session actor from a TCP stream
    fn create_session(stream: TcpStream, session_type: SessionType) {
        // Create a session actor
        Session::create(move |ctx| {
            // TODO: handle error
            let address = stream.peer_addr().unwrap();

            // Split TCP stream into read and write parts
            let (r, w) = stream.split();

            // Add stream in session actor from the read part of the tcp stream
            Session::add_stream(FramedRead::new(r, P2PCodec), ctx);

            // Create the session actor and store in its state the write part of the tcp stream
            Session::new(address, session_type, FramedWrite::new(w, P2PCodec, ctx))
        });
    }

    /// Method to process resolver ConnectAddr response
    fn process_connect_addr_response(
        response: Result<Result<TcpStream, ResolverError>, MailboxError>,
    ) -> FutureResult<(), (), Self> {
        match response {
            Ok(result) => {
                match result {
                    Ok(stream) => {
                        info!("Connected to peer {:?}", stream.peer_addr());

                        // Create a session actor from connection
                        ConnectionsManager::create_session(stream, SessionType::Outbound);

                        actix::fut::ok(())
                    }
                    Err(e) => {
                        info!("Error while trying to connect to the peer: {}", e);
                        actix::fut::err(())
                    }
                }
            }
            Err(_) => {
                info!("Unsuccessful communication with resolver");
                actix::fut::err(())
            }
        }
    }
}

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR MESSAGE HANDLERS
////////////////////////////////////////////////////////////////////////////////////////
/// Handler for InboundTcpConnect messages (built from inbound connections)
impl Handler<InboundTcpConnect> for ConnectionsManager {
    /// Response for message, which is defined by `ResponseType` trait
    type Result = ();

    /// Method to handle the InboundTcpConnect message
    fn handle(&mut self, msg: InboundTcpConnect, _ctx: &mut Self::Context) {
        // Create a session actor from connection
        ConnectionsManager::create_session(msg.stream, SessionType::Inbound);
    }
}

/// Handler for OutboundTcpConnect messages (requested for creating outgoing connections)
impl Handler<OutboundTcpConnect> for ConnectionsManager {
    /// Response for message, which is defined by `ResponseType` trait
    type Result = ();

    /// Method to handle the OutboundTcpConnect message
    fn handle(&mut self, msg: OutboundTcpConnect, ctx: &mut Self::Context) {
        // Get resolver from registry and send a ConnectAddr message to it
        Resolver::from_registry()
            .send(ConnectAddr(msg.address))
            .into_actor(self)
            .then(|res, _act, _ctx| ConnectionsManager::process_connect_addr_response(res))
            .wait(ctx);
    }
}
