use crate::output::OutputAdapter;
use amqp_lapin_helper::{Broker, BrokerListener, Delivery};
use async_trait::async_trait;
use std::error::Error;

#[async_trait]
impl OutputAdapter for AmqpOutput {
    async fn send(&self, line: String) -> Result<(), Box<dyn Error>> {
        println!("got = {}", line);

        // confirm ack is not used, shall we use it?
        let _confirm = self
            .publisher
            .publish_raw(&self.exchange, &self.routing_key, line.as_bytes().to_vec())
            .await?;

        Ok(())
    }
}

pub struct AmqpOutput {
    publisher: amqp_lapin_helper::Publisher,
    exchange: String,
    routing_key: String,
}

struct ToBeRemoved {}

/// TODO to be removed
#[async_trait]
impl BrokerListener for ToBeRemoved {
    fn exchange_name(&self) -> &'static str {
        todo!()
    }

    async fn consume(&self, delivery: Delivery) -> amqp_lapin_helper::Result<()> {
        todo!()
    }
}

impl AmqpOutput {
    pub async fn new(uri: &str, exchange: &str, routing_key: &str) -> Result<Self, Box<dyn Error>> {
        // init the broker
        let mut broker: Broker<ToBeRemoved> = Broker::new();
        broker.init(uri).await?;

        // Set the publisher up then clone it
        let publisher = broker.setup_publisher().await?.clone();

        Ok(Self {
            publisher,
            exchange: exchange.to_owned(),
            routing_key: routing_key.to_owned(),
        })
    }
}
